//! Implementation of the `qmd collection` subcommands (list/add/remove/rename/show).
//!
//! Logic ported verbatim from the original main.rs implementation during modularization.
//! Preserves all YAML mutation, DB sync, and user messaging exactly.

use crate::cli::args::CollectionAction;
use crate::db::{
    get_collection_stats, load_config, load_config_value, open_connection, save_config_value,
};
use anyhow::Result;
use std::env;

/// Handle the `qmd collection <action>` commands (full parity for YAML + store_collections).
pub fn cmd_collection(action: CollectionAction) -> Result<()> {
    match action {
        CollectionAction::List => {
            let cfg = load_config().unwrap_or_default();
            let cols = cfg.collections.unwrap_or_default();
            if cols.is_empty() {
                println!("No collections found. Run 'qmd collection add <path>' to create one.");
                return Ok(());
            }
            println!("Collections ({}):", cols.len());
            println!();
            for (name, c) in &cols {
                let (fcount, last) = get_collection_stats(name);
                println!("{} (qmd://{}/)", name, name);
                println!("  Pattern:  {}", c.pattern);
                if let Some(ips) = &c.ignore_patterns {
                    if !ips.is_empty() {
                        println!("  Ignore:   {}", ips.join(", "));
                    }
                }
                println!("  Files:    {}", fcount);
                println!("  Path:     {}", c.path);
                if !last.is_empty() && last != "unknown" {
                    println!("  Updated:  {}", last);
                }
                println!();
            }
        }
        CollectionAction::Add { path, name, mask } => {
            let p = path.to_string_lossy().to_string();
            let resolved = if p == "." {
                env::current_dir()?.to_string_lossy().to_string()
            } else {
                p
            };
            let coll_name = name.unwrap_or_else(|| {
                std::path::Path::new(&resolved)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("root")
                    .to_string()
            });
            let pattern = mask.unwrap_or_else(|| "**/*.md".to_string());

            let mut v = load_config_value()?;
            {
                let root_map = match v.as_mapping_mut() {
                    Some(m) => m,
                    None => {
                        eprintln!("invalid config root (expected mapping)");
                        std::process::exit(1);
                    }
                };
                let cols_val = root_map
                    .entry(serde_yaml::Value::String("collections".into()))
                    .or_insert(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                if let Some(m) = cols_val.as_mapping_mut() {
                    let key = serde_yaml::Value::String(coll_name.clone());
                    if m.contains_key(&key) {
                        eprintln!("Collection '{}' already exists.", coll_name);
                        std::process::exit(1);
                    }
                    let mut entry = serde_yaml::Mapping::new();
                    entry.insert(
                        serde_yaml::Value::String("path".into()),
                        serde_yaml::Value::String(resolved.clone()),
                    );
                    entry.insert(
                        serde_yaml::Value::String("pattern".into()),
                        serde_yaml::Value::String(pattern.clone()),
                    );
                    m.insert(key, serde_yaml::Value::Mapping(entry));
                }
            }
            eprintln!("Warning: collection mutation rewrites YAML and strips comments (use the reference implementation for comment-preserving edits if needed).");
            save_config_value(&v)?;

            // keep store_collections in sync (no full index, just metadata)
            if let Ok(conn) = open_connection(false) {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO store_collections(name, path, pattern) VALUES(?, ?, ?)",
                    rusqlite::params![&coll_name, &resolved, &pattern],
                );
            }

            println!("✓ Collection '{}' added (qmd://{}/)", coll_name, coll_name);
            println!("  Path: {}", resolved);
            println!("  Pattern: {}", pattern);
            println!("  Note: files not indexed yet. Use `qmd update` (or the reference implementation) for indexing.");
        }
        CollectionAction::Remove { name } => {
            let mut v = load_config_value()?;
            let existed = if let Some(cols) = v
                .as_mapping_mut()
                .and_then(|m| m.get_mut("collections"))
                .and_then(|c| c.as_mapping_mut())
            {
                cols.remove(serde_yaml::Value::String(name.clone()))
                    .is_some()
            } else {
                false
            };
            if !existed {
                eprintln!("Collection not found: {}", name);
                std::process::exit(1);
            }
            eprintln!("Warning: collection mutation rewrites YAML and strips comments (use the reference implementation for comment-preserving edits if needed).");
            save_config_value(&v)?;

            let deact = if let Ok(conn) = open_connection(false) {
                let d = conn
                    .execute(
                        "UPDATE documents SET active=0 WHERE collection = ? AND active=1",
                        [&name],
                    )
                    .unwrap_or(0) as u32;
                let _ = conn.execute("DELETE FROM store_collections WHERE name = ?", [&name]);
                d
            } else {
                0
            };
            println!("✓ Removed collection '{}'", name);
            println!(
                "  Deactivated {} documents (DB). Run Node cleanup if needed.",
                deact
            );
        }
        CollectionAction::Rename { old, new } => {
            let mut v = load_config_value()?;
            let ok = if let Some(cols) = v
                .as_mapping_mut()
                .and_then(|m| m.get_mut("collections"))
                .and_then(|c| c.as_mapping_mut())
            {
                if cols.contains_key(serde_yaml::Value::String(new.clone())) {
                    eprintln!("Target collection '{}' already exists.", new);
                    std::process::exit(1);
                }
                if let Some(entry) = cols.remove(serde_yaml::Value::String(old.clone())) {
                    cols.insert(serde_yaml::Value::String(new.clone()), entry);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !ok {
                eprintln!("Collection not found: {}", old);
                std::process::exit(1);
            }
            eprintln!("Warning: collection mutation rewrites YAML and strips comments (use the reference implementation for comment-preserving edits if needed).");
            save_config_value(&v)?;

            if let Ok(conn) = open_connection(false) {
                let _ = conn.execute(
                    "UPDATE store_collections SET name = ? WHERE name = ?",
                    [&new, &old],
                );
                let _ = conn.execute(
                    "UPDATE documents SET collection = ? WHERE collection = ?",
                    [&new, &old],
                );
            }
            println!("✓ Renamed '{}' -> '{}'", old, new);
        }
        CollectionAction::Show { name } => {
            let cfg = load_config().unwrap_or_default();
            let cols = cfg.collections.unwrap_or_default();
            if let Some(c) = cols.get(&name) {
                let (fcount, last) = get_collection_stats(&name);
                println!("Collection: {}", name);
                println!("  Path:     {}", c.path);
                println!("  Pattern:  {}", c.pattern);
                if let Some(ips) = &c.ignore_patterns {
                    if !ips.is_empty() {
                        println!("  Ignore:   {}", ips.join(", "));
                    }
                }
                println!("  Files:    {}", fcount);
                println!("  Updated:  {}", last);
                return Ok(());
            }
            eprintln!("Collection not found: {}", name);
            std::process::exit(1);
        }
    }
    Ok(())
}
