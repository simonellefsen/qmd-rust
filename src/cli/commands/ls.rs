//! Implementation of the `qmd ls` command (lists collections or files under qmd:// paths).
//!
//! Supports bare names, collection/path, and qmd:// virtual syntax. Uses DB queries for active docs.

use crate::db::{load_config, open_connection};
use anyhow::Result;

use super::{escape_like, parse_qmd_virtual};

/// Handle `qmd ls [path]`
pub fn cmd_ls(path: Option<String>) -> Result<()> {
    if path.as_deref().unwrap_or("").trim().is_empty() {
        let cfg = load_config().unwrap_or_default();
        let cols = cfg.collections.unwrap_or_default();
        if cols.is_empty() {
            println!("No collections. Run 'qmd collection add .' ");
            return Ok(());
        }
        println!("Collections:");
        for name in cols.keys() {
            let cnt = if let Ok(conn) = open_connection(true) {
                conn.query_row(
                    "SELECT COUNT(*) FROM documents WHERE collection=? AND active=1",
                    [name],
                    |r| r.get::<_, u32>(0),
                )
                .unwrap_or(0)
            } else {
                0
            };
            println!("  qmd://{}/  ({} files)", name, cnt);
        }
        return Ok(());
    }

    // Safe: the if-guard above returns early for None/empty-trim cases (pre-existing control-flow pattern from monolithic main.rs).
    let p = path.unwrap();
    let (coll_name, prefix_opt) = if let Some((c, r)) = parse_qmd_virtual(&p) {
        (c, if r.is_empty() { None } else { Some(r) })
    } else if p.contains('/') && !p.starts_with('/') && !p.starts_with('~') {
        let mut it = p.splitn(2, '/');
        (
            it.next().unwrap_or("").to_string(),
            it.next().map(|s| s.to_string()),
        )
    } else {
        (p, None)
    };

    if coll_name.is_empty() {
        println!("Invalid path for ls");
        return Ok(());
    }

    let like = prefix_opt
        .as_ref()
        .map(|pr| format!("{}%", escape_like(pr)))
        .unwrap_or_else(|| "%".to_string());

    let mut files: Vec<String> = Vec::new();
    if let Ok(conn) = open_connection(true) {
        if prefix_opt.is_some() {
            let sql = "SELECT path FROM documents WHERE collection = ? AND path LIKE ? ESCAPE '\\' AND active=1 ORDER BY path";
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([&coll_name, &like], |r| r.get::<_, String>(0))?;
            for fp in rows.flatten() {
                files.push(fp);
            }
        } else {
            let sql = "SELECT path FROM documents WHERE collection = ? AND active=1 ORDER BY path";
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([&coll_name], |r| r.get::<_, String>(0))?;
            for fp in rows.flatten() {
                files.push(fp);
            }
        }
    }

    if files.is_empty() {
        println!(
            "No files under qmd://{}/{}",
            coll_name,
            prefix_opt.unwrap_or_default()
        );
    } else {
        println!("qmd://{}/ :", coll_name);
        for f in files {
            println!("  {}", f);
        }
    }
    Ok(())
}
