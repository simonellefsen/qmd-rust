//! Database and configuration layer for QMD-Rust.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct QmdConfig {
    pub collections: Option<HashMap<String, CollectionCfg>>,
    pub models: Option<ModelsCfg>,
    /// Editor URI template for TTY clickable hyperlinks in search/query/get output.
    /// Supports placeholders {path}, {line}, {col}. Resolved from QMD_EDITOR_URI env
    /// (higher priority) or `editor_uri` key at root of the active index config
    /// (index.yml / index.yaml, preferring local .qmd/ when present).
    #[serde(default)]
    pub editor_uri: Option<String>,
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

/// Walk upward from CWD looking for a `.qmd/` directory. If present, the CLI
/// prefers the project-local index (index.yml + index.sqlite inside it).
/// This is the minimal detection logic for `qmd init` and local-tree preference.
fn find_local_qmd_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(".qmd");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Return the active config path, preferring a local `.qmd/index.yml` (or .yaml)
/// when the process CWD is inside a tree that has a `.qmd/` directory.
pub fn active_config_path() -> PathBuf {
    if let Some(root) = find_local_qmd_root() {
        let yml = root.join("index.yml");
        let yaml = root.join("index.yaml");
        if yaml.exists() && !yml.exists() {
            return yaml;
        }
        return yml;
    }
    PathBuf::from(expand_tilde("~/.config/qmd/index.yml"))
}

/// Return the active SQLite path, preferring the one inside a local `.qmd/`
/// when detected.
pub fn active_db_path() -> String {
    if let Some(root) = find_local_qmd_root() {
        return root.join("index.sqlite").to_string_lossy().into_owned();
    }
    expand_tilde("~/.cache/qmd/index.sqlite")
}

pub fn load_config() -> Result<QmdConfig> {
    let path = active_config_path();
    if !path.exists() {
        return Ok(QmdConfig::default());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&text)
        .with_context(|| format!("failed to parse YAML at {}", path.display()))
}

pub fn open_connection(read_only: bool) -> Result<Connection> {
    let expanded = active_db_path();
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

/// Returns embedding vector chunk count for one collection (via hash join to content_vectors).
/// Follows *exact* pattern of get_collection_stats (open_connection read-only, query_row, graceful 0 on error/empty).
/// Used for per-collection embedding health in status (pairs with doc count for coverage signal).
/// For Rust newbies: Option/Result from conn ops turn into 0 via if-let + unwrap_or; no panics on missing tables (sqlite returns 0 for COUNT on absent? but IF NOT in bootstrap).
pub fn collection_vector_count(name: &str) -> u32 {
    if let Ok(conn) = open_connection(true) {
        if let Ok(cnt) = conn.query_row(
            "SELECT COUNT(*) FROM content_vectors cv JOIN documents d ON d.hash = cv.hash WHERE d.collection = ? AND d.active = 1",
            [name],
            |r| r.get(0),
        ) {
            return cnt;
        }
    }
    0
}

pub fn load_config_value() -> Result<serde_yaml::Value> {
    let path = active_config_path();
    if !path.exists() {
        let mut m = serde_yaml::Mapping::new();
        m.insert("collections".into(), serde_yaml::Mapping::new().into());
        return Ok(m.into());
    }
    let text = fs::read_to_string(&path)?;
    serde_yaml::from_str(&text).context("failed to parse config")
}

pub fn save_config_value(v: &serde_yaml::Value) -> Result<()> {
    let path = active_config_path();
    if let Some(d) = path.parent() {
        let _ = fs::create_dir_all(d);
    }
    fs::write(&path, serde_yaml::to_string(v)?)?;
    Ok(())
}

/// For Rust newbies coming from Python/TS:
/// - `Option<String>`: may be absent (None) or present (Some(value)); use .is_some(), .unwrap_or etc.
/// - env::var returns Result; .ok() turns Err into None for graceful fallback.
/// - We check env first (QMD_EDITOR_URI), then config; empty strings treated as absent.
///
///   This keeps agent/LLM-wiki TTY flows working even if config is partial.
pub fn editor_uri() -> Option<String> {
    if let Ok(v) = std::env::var("QMD_EDITOR_URI") {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let cfg = load_config().ok()?;
    cfg.editor_uri
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Resolve a hit "file" field (usually "qmd://coll/rel/path.md" from search results,
/// or bare FS path) into a best-effort absolute filesystem path for editor links.
/// Uses collections config for qmd:// resolution + expand_tilde.
/// Per-entry graceful degradation: on any parse/lookup failure, returns the input
/// (or stripped qmd:// form) so one bad hit never breaks output for the rest.
/// (Matches "error handling in .../path code must degrade gracefully".)
pub fn resolve_document_fs_path(hit_file: &str) -> String {
    if !hit_file.starts_with("qmd://") {
        return expand_tilde(hit_file);
    }
    // Tiny inline parser (dupe of commands/mod parse_qmd_virtual) to avoid module cycle.
    // For Rust newbies: splitn(2, '/') gives at most 2 parts; ? early-returns None on missing.
    let s = hit_file
        .trim_start_matches("qmd:")
        .trim_start_matches('/')
        .trim_start_matches('/');
    if s.is_empty() {
        return hit_file.to_string();
    }
    let mut it = s.splitn(2, '/');
    let coll = match it.next() {
        Some(c) if !c.is_empty() => c.to_string(),
        _ => return hit_file.to_string(),
    };
    let rel = it.next().unwrap_or("").to_string();

    // Lookup in config; if missing or error, graceful fallback to original form.
    if let Ok(cfg) = load_config() {
        if let Some(colls) = cfg.collections {
            if let Some(c) = colls.get(&coll) {
                let base = expand_tilde(&c.path);
                let joined = if rel.is_empty() {
                    base
                } else if base.ends_with('/') {
                    format!("{}{}", base, rel)
                } else {
                    format!("{}/{}", base, rel)
                };
                return joined;
            }
        }
    }
    // Graceful: return a usable form (strip qmd: prefix) rather than failing whole result set.
    hit_file
        .trim_start_matches("qmd:")
        .trim_start_matches('/')
        .to_string()
}

/// Build a concrete editor open URI by substituting into the template from editor_uri().
/// {path} gets the resolved FS path; {line}/{col} default to 1 if absent (current hits
/// carry no per-chunk line data; future slice can extend FtsHit + queries).
/// Returns None if no template configured (caller decides plain vs link).
pub fn build_editor_uri(hit_file: &str, line: Option<u32>, col: Option<u32>) -> Option<String> {
    let tpl = editor_uri()?;
    let fs = resolve_document_fs_path(hit_file);
    let l = line.unwrap_or(1);
    let c = col.unwrap_or(1);
    let uri = tpl
        .replace("{path}", &fs)
        .replace("{line}", &l.to_string())
        .replace("{col}", &c.to_string());
    Some(uri)
}

/// Returns true when stdout is attached to a terminal (TTY).
/// Used to decide whether to emit OSC 8 hyperlinks (harmless in pipes/files/JSON).
/// For Rust newbies: IsTerminal trait on std::io::Stdout since Rust 1.70; the
/// method is cheap and the reason we don't always hyperlink (would pollute --json etc).
pub fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Format a result path (from hits or disk fallback) for console output.
/// - If not a TTY or no editor_uri configured: plain text (original hit_file value).
/// - Else: OSC-8 hyperlink with href=built editor URI; visible text = original hit_file
///   (preserves qmd:// style users see today, while click opens real file in editor).
///
///   The Result/Option handling ensures a bad template or resolve never panics a result row.
///   (See AGENTS.md for Rust-newbie context on Option/Result/? .)
pub fn format_path_for_output(hit_file: &str, line: Option<u32>, col: Option<u32>) -> String {
    if !stdout_is_tty() {
        return hit_file.to_string();
    }
    match build_editor_uri(hit_file, line, col) {
        Some(uri) => {
            // OSC 8 hyperlink syntax (widely supported in modern terminals/editors):
            // \x1b]8;;URI\x1b\   visible-text   \x1b]8;;\x1b\
            // (the \\ in Rust source emits a single backslash char)
            let esc = "\x1b";
            let osc_start = format!("{}]8;;{}{}\\", esc, uri, esc);
            let osc_end = format!("{}]8;;{}{}", esc, esc, "\\");
            format!("{}{}{}", osc_start, hit_file, osc_end)
        }
        None => hit_file.to_string(),
    }
}

pub mod search;
pub use search::{build_fts5_query, fts_search, vec_search, FtsHit};
