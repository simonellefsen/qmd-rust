//! `qmd context` subcommands (add/list/rm/check).
//!
//! Extracted from mod.rs in the review fix round (to resolve the high-impact
//! "monolith in mod.rs" finding while obeying the original task's "never create
//! files unless absolutely necessary" + "smallest viable" constraints for the
//! main implementation pass). The per-file layout documented in the module
//! header, lib.rs contributor guide, and wiki/runbooks is now followed.
//!
//! For Rust newbies: same load_config_value + manual Mapping mutation pattern
//! as collection.rs (so unknown YAML keys and as much structure as possible are
//! preserved on write). qmd:// support reuses the shared parser. All error
//! paths and output messages kept as close as possible to the prior working
//! implementation.
//!
//! Fixes incorporated in this extraction slice (review round):
//! - Bug #2: existence check on the target collection before any mutation
//!   (prevents partial/stub collection records that would corrupt the index.yml).
//! - #4 (warning regression): path-free comment-stripping warning emitted on
//!   mutations (no external paths introduced).
//! - #7 (duplication): tiny private helper for the repeated prefix normalization.
//! - Check variant remains (but hidden at the clap layer in args.rs for parity).

use crate::cli::args::ContextAction;
use crate::db::{load_config, load_config_value, save_config_value};
use anyhow::Result;

/// Small helper to deduplicate the prefix normalization logic used by both Add
/// and Rm (addresses review finding #7 with minimal targeted change inside the
/// now-properly-located file).
fn normalize_context_prefix(r: &str) -> String {
    if r.is_empty() || r == "/" {
        "/".to_string()
    } else if r.starts_with('/') {
        r.to_string()
    } else {
        format!("/{}", r)
    }
}

pub fn cmd_context(action: ContextAction) -> Result<()> {
    match action {
        ContextAction::Add { path, text } => {
            let txt = text.join(" ").trim().to_string();
            if txt.is_empty() {
                eprintln!("Usage: qmd context add [path] \"text\"");
                eprintln!("Examples: qmd context add qmd://MyColl/ \"summary of MyColl\"");
                eprintln!("          qmd context add / \"global note for all\"");
                return Ok(());
            }
            let p = path.unwrap_or_default();
            let mut v = load_config_value()?;
            if p == "/" || p.is_empty() {
                if let Some(m) = v.as_mapping_mut() {
                    m.insert(
                        serde_yaml::Value::String("global_context".into()),
                        serde_yaml::Value::String(txt.clone()),
                    );
                }
                save_config_value(&v)?;
                println!("✓ Set global context");
                println!("  Context: {}", txt);
                return Ok(());
            }
            // Resolve collection + path_prefix (prefer explicit qmd://, fall back to coll/path form)
            let (coll, pfx) = if let Some((c, r)) = super::parse_qmd_virtual(&p) {
                (c, normalize_context_prefix(&r))
            } else if p.contains('/') && !p.starts_with('/') && !p.starts_with('~') {
                // bare coll/rel form
                let mut it = p.splitn(2, '/');
                let c = it.next().unwrap_or("").to_string();
                let r = it.next().unwrap_or("").to_string();
                (c, normalize_context_prefix(&r))
            } else {
                eprintln!("Unsupported path form for context (use qmd://coll/path or coll/path or / for global).");
                std::process::exit(1);
            };
            if coll.is_empty() {
                eprintln!("Invalid collection in path: {}", p);
                std::process::exit(1);
            }

            // === Bug #2 fix (review round): existence check before any mutation ===
            // This prevents auto-creating stub collection entries missing the required
            // `path` + `pattern` fields (which would corrupt the config for the typed
            // CollectionCfg loader and for `qmd update` / status). Matches the guard in
            // original yamlAddContext + the check in collection.rs:74.
            {
                let cfg = load_config().unwrap_or_default();
                let exists = cfg
                    .collections
                    .as_ref()
                    .is_some_and(|m| m.contains_key(&coll));
                if !exists {
                    eprintln!("Collection not found: {}", coll);
                    std::process::exit(1);
                }
            }

            {
                let root = match v.as_mapping_mut() {
                    Some(m) => m,
                    None => {
                        eprintln!("config root is not a mapping");
                        std::process::exit(1);
                    }
                };
                let cols_val = root
                    .entry(serde_yaml::Value::String("collections".into()))
                    .or_insert(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                if let Some(cols_m) = cols_val.as_mapping_mut() {
                    let ckey = serde_yaml::Value::String(coll.clone());
                    let coll_entry = cols_m
                        .entry(ckey.clone())
                        .or_insert(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                    if let Some(cm) = coll_entry.as_mapping_mut() {
                        let ctx_val = cm
                            .entry(serde_yaml::Value::String("context".into()))
                            .or_insert(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                        if let Some(ctx_m) = ctx_val.as_mapping_mut() {
                            ctx_m.insert(
                                serde_yaml::Value::String(pfx.clone()),
                                serde_yaml::Value::String(txt.clone()),
                            );
                        }
                    }
                }
            }
            // Path-free warning (addresses review #4; no external paths in new text)
            eprintln!(
                "Warning: context mutation rewrites YAML and may strip comments and formatting."
            );
            save_config_value(&v)?;
            let display = if pfx == "/" {
                format!("qmd://{}/", coll)
            } else {
                format!("qmd://{}/{}", coll, pfx.trim_start_matches('/'))
            };
            println!("✓ Added context for: {}", display);
            println!("  Context: {}", txt);
        }
        ContextAction::List => {
            let v = load_config_value()?;
            let mut items: Vec<(String, String, String)> = Vec::new();
            if let Some(m) = v.as_mapping() {
                if let Some(g) = m.get("global_context").and_then(|x| x.as_str()) {
                    if !g.trim().is_empty() {
                        items.push(("*".to_string(), "/".to_string(), g.to_string()));
                    }
                }
                if let Some(cols) = m.get("collections").and_then(|c| c.as_mapping()) {
                    for (name, collv) in cols {
                        let name_s = name.as_str().unwrap_or_default().to_string();
                        if let Some(ctx) = collv.get("context").and_then(|c| c.as_mapping()) {
                            for (pth, txtv) in ctx {
                                let pth_s = pth.as_str().unwrap_or_default().to_string();
                                let txt_s = txtv.as_str().unwrap_or_default().to_string();
                                if !txt_s.trim().is_empty() {
                                    items.push((name_s.clone(), pth_s, txt_s));
                                }
                            }
                        }
                    }
                }
            }
            if items.is_empty() {
                println!("No contexts configured. Use 'qmd context add' to add one.");
                return Ok(());
            }
            println!("\nConfigured Contexts\n");
            let mut last = String::new();
            for (coll, pth, txt) in items {
                if coll != last {
                    println!("{}", coll);
                    last = coll;
                }
                let disp = if pth == "/" || pth.is_empty() {
                    "  / (root)".to_string()
                } else {
                    format!("  {}", pth)
                };
                println!("{}", disp);
                println!("    {}", txt);
            }
        }
        ContextAction::Rm { path } => {
            let p = path;
            let mut v = load_config_value()?;
            let mut removed = false;
            if p == "/" {
                if let Some(m) = v.as_mapping_mut() {
                    removed = m.remove("global_context").is_some();
                }
            } else {
                let (coll, pfx) = if let Some((c, r)) = super::parse_qmd_virtual(&p) {
                    (c, normalize_context_prefix(&r))
                } else if p.contains('/') && !p.starts_with('/') && !p.starts_with('~') {
                    let mut it = p.splitn(2, '/');
                    let c = it.next().unwrap_or("").to_string();
                    let r = it.next().unwrap_or("").to_string();
                    (c, normalize_context_prefix(&r))
                } else {
                    eprintln!("Unsupported path for rm (use qmd://coll/... or / or coll/path)");
                    std::process::exit(1);
                };
                if let Some(cols) = v
                    .as_mapping_mut()
                    .and_then(|m| m.get_mut("collections"))
                    .and_then(|c| c.as_mapping_mut())
                {
                    if let Some(collm) = cols.get_mut(serde_yaml::Value::String(coll.clone())) {
                        if let Some(cm) = collm.as_mapping_mut() {
                            if let Some(ctxv) = cm.get_mut("context") {
                                if let Some(ctxm) = ctxv.as_mapping_mut() {
                                    removed = ctxm
                                        .remove(serde_yaml::Value::String(pfx.clone()))
                                        .is_some();
                                    if ctxm.is_empty() {
                                        cm.remove("context");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !removed {
                eprintln!("No context found for: {}", p);
                std::process::exit(1);
            }
            // Path-free warning (addresses review #4)
            eprintln!(
                "Warning: context mutation rewrites YAML and may strip comments and formatting."
            );
            save_config_value(&v)?;
            println!("✓ Removed context for: {}", p);
        }
        ContextAction::Check => {
            // Smallest audit: report collections (from config) that have no context entries.
            // (Full DB-backed top-level path audit was removed in reference; this is the
            // viable equivalent without extra store logic.)
            let v = load_config_value()?;
            let mut without: Vec<String> = Vec::new();
            if let Some(cols) = v.get("collections").and_then(|c| c.as_mapping()) {
                for (name, collv) in cols {
                    let has = collv
                        .get("context")
                        .and_then(|c| c.as_mapping())
                        .map(|m| !m.is_empty())
                        .unwrap_or(false);
                    if !has {
                        without.push(name.as_str().unwrap_or_default().to_string());
                    }
                }
            }
            println!("context check — collections without any context:");
            if without.is_empty() {
                println!("  (all collections have at least one context entry, or none configured)");
            } else {
                for w in &without {
                    println!("  - {}", w);
                }
                println!(
                    "  Tip: qmd context add qmd://{}/ \"description of the collection\"",
                    without[0]
                );
            }
            // Also note global if present
            if let Some(g) = v.get("global_context").and_then(|x| x.as_str()) {
                if !g.trim().is_empty() {
                    println!("(global_context is set)");
                }
            }
        }
    }
    Ok(())
}
