//! Database and configuration layer for QMD-Rust.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct QmdConfig {
    pub collections: Option<HashMap<String, CollectionCfg>>,
    pub models: Option<ModelsCfg>,
}

#[derive(Debug, Deserialize)]
pub struct CollectionCfg {
    pub path: String,
    #[serde(default = "default_pattern")]
    pub pattern: String,
    /// Per-collection ignore globs (supports `ignore:` or `ignore_patterns:` in YAML for compatibility)
    #[serde(default, alias = "ignore", alias = "ignore_patterns")]
    pub ignore_patterns: Option<Vec<String>>,
}

fn default_pattern() -> String {
    "**/*.md".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct ModelsCfg {
    pub embed: Option<String>,
    pub generate: Option<String>,
    pub rerank: Option<String>,
}

pub fn expand_tilde(p: &str) -> String {
    if let Some(home) = env::var_os("HOME") {
        if let Some(stripped) = p.strip_prefix("~/") {
            return format!("{}/{}", home.to_string_lossy(), stripped);
        }
    }
    p.to_string()
}

pub fn load_config() -> Result<QmdConfig> {
    let path = PathBuf::from(expand_tilde("~/.config/qmd/index.yml"));
    if !path.exists() {
        return Ok(QmdConfig::default());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&text)
        .with_context(|| format!("failed to parse YAML at {}", path.display()))
}

pub fn open_connection(read_only: bool) -> Result<Connection> {
    let expanded = expand_tilde("~/.cache/qmd/index.sqlite");
    if !read_only {
        if let Some(parent) = std::path::Path::new(&expanded).parent() {
            let _ = fs::create_dir_all(parent);
        }
    }
    let flags = if read_only {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
    };
    Connection::open_with_flags(&expanded, flags)
        .with_context(|| format!("failed to open DB at {}", expanded))
}

pub fn db_counts(db_path: &str) -> Option<(u32, u32)> {
    let expanded = expand_tilde(db_path);
    let conn =
        Connection::open_with_flags(&expanded, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let doc: u32 = conn
        .query_row("SELECT COUNT(*) FROM documents WHERE active=1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let vec: u32 = conn
        .query_row("SELECT COUNT(*) FROM content_vectors", [], |r| r.get(0))
        .unwrap_or(0);
    Some((doc, vec))
}

pub fn last_updated_hint(db_path: &str) -> Option<String> {
    let expanded = expand_tilde(db_path);
    let conn =
        Connection::open_with_flags(&expanded, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let ts: String = conn
        .query_row(
            "SELECT COALESCE(MAX(modified_at), '') FROM documents WHERE active=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if ts.is_empty() {
        None
    } else {
        Some(ts)
    }
}

pub fn get_collection_stats(name: &str) -> (u32, String) {
    if let Ok(conn) = open_connection(true) {
        if let Ok((cnt, last)) = conn.query_row(
            "SELECT COUNT(*), COALESCE(MAX(modified_at), '') FROM documents WHERE collection = ? AND active = 1",
            [name],
            |r| Ok((r.get::<_, u32>(0).unwrap_or(0), r.get::<_, String>(1).unwrap_or_default())),
        ) {
            return (cnt, last);
        }
    }
    (0, "unknown".to_string())
}

pub fn load_config_value() -> Result<serde_yaml::Value> {
    let path = PathBuf::from(expand_tilde("~/.config/qmd/index.yml"));
    if !path.exists() {
        let mut m = serde_yaml::Mapping::new();
        m.insert("collections".into(), serde_yaml::Mapping::new().into());
        return Ok(m.into());
    }
    let text = fs::read_to_string(&path)?;
    serde_yaml::from_str(&text).context("failed to parse config")
}

pub fn save_config_value(v: &serde_yaml::Value) -> Result<()> {
    let path = PathBuf::from(expand_tilde("~/.config/qmd/index.yml"));
    if let Some(d) = path.parent() {
        let _ = fs::create_dir_all(d);
    }
    fs::write(&path, serde_yaml::to_string(v)?)?;
    Ok(())
}

pub mod search;
pub use search::{build_fts5_query, fts_search, vec_search, FtsHit};
