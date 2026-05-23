//! Database and configuration layer for QMD-Rust.
//!
//! This module is the home for all SQLite + YAML persistence logic.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

// Re-export the main types used by the CLI and commands
pub use self::config_types::{QmdConfig, CollectionCfg, ModelsCfg};

mod config_types {
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Debug, Deserialize, Default)]
    pub struct QmdConfig {
        pub collections: Option<HashMap<String, CollectionCfg>>,
        pub models: Option<ModelsCfg>,
    }

    #[derive(Debug, Deserialize)]
    pub struct CollectionCfg {
        pub path: String,
        #[serde(default = "super::default_pattern")]
        pub pattern: String,
    }

    #[derive(Debug, Deserialize, Default)]
    pub struct ModelsCfg {
        pub embed: Option<String>,
        pub generate: Option<String>,
        pub rerank: Option<String>,
    }
}

pub fn default_pattern() -> String {
    "**/*.md".to_string()
}

/// Expand `~/foo` → absolute path
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
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let cfg: QmdConfig = serde_yaml::from_str(&text)
        .with_context(|| format!("failed to parse YAML at {}", path.display()))?;
    Ok(cfg)
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
    let conn = Connection::open_with_flags(&expanded, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let doc_count: u32 = conn
        .query_row("SELECT COUNT(*) FROM documents WHERE active=1", [], |r| r.get(0))
        .unwrap_or(0);
    let vec_count: u32 = conn
        .query_row("SELECT COUNT(*) FROM content_vectors", [], |r| r.get(0))
        .unwrap_or(0);
    Some((doc_count, vec_count))
}

pub fn last_updated_hint(db_path: &str) -> Option<String> {
    let expanded = expand_tilde(db_path);
    let conn = Connection::open_with_flags(&expanded, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let ts: String = conn
        .query_row(
            "SELECT COALESCE(MAX(modified_at), '') FROM documents WHERE active=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if ts.is_empty() { None } else { Some(ts) }
}

pub fn get_collection_stats(name: &str) -> (u32, String) {
    if let Ok(conn) = open_connection(true) {
        if let Ok((cnt, last)) = conn.query_row(
            "SELECT COUNT(*) , COALESCE(MAX(modified_at), '') FROM documents WHERE collection = ? AND active = 1",
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
        let mut root = serde_yaml::Mapping::new();
        root.insert(
            serde_yaml::Value::String("collections".into()),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
        return Ok(serde_yaml::Value::Mapping(root));
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let v: serde_yaml::Value = serde_yaml::from_str(&text)
        .with_context(|| format!("failed to parse YAML at {}", path.display()))?;
    Ok(v)
}

pub fn save_config_value(v: &serde_yaml::Value) -> Result<()> {
    let path = PathBuf::from(expand_tilde("~/.config/qmd/index.yml"));
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let text = serde_yaml::to_string(v).context("failed to serialize config to YAML")?;
    fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
