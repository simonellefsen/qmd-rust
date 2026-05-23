//! Individual command implementations (cmd_status, cmd_search, cmd_get, etc.)
//!
//! Each command lives in its own file for maintainability.
//! Shared path/FS helpers for ls/get/mcp live here (pub(crate) so submodules can `use super::` them).

pub mod collection;
pub mod get;
pub mod ls;
pub mod mcp;
pub mod search;
pub mod status;

use crate::db::open_connection;

/// Parse a qmd://... or similar virtual path into (collection, rest_path).
pub(crate) fn parse_qmd_virtual(p: &str) -> Option<(String, String)> {
    let s = p
        .trim_start_matches("qmd:")
        .trim_start_matches('/')
        .trim_start_matches('/');
    if s.is_empty() {
        return None;
    }
    let mut it = s.splitn(2, '/');
    let coll = it.next()?.to_string();
    let rest = it.next().unwrap_or("").to_string();
    if coll.is_empty() {
        return None;
    }
    Some((coll, rest))
}

/// Escape SQL LIKE wildcards so user paths containing % or _ do not over-match (addresses latent bug in prefix/suffix queries).
pub(crate) fn escape_like(p: &str) -> String {
    p.replace('%', "\\%").replace('_', "\\_")
}

pub(crate) fn get_body_from_db(target: &str) -> Option<String> {
    let conn = open_connection(true).ok()?;
    // qmd:// or virtual
    if let Some((coll, pth)) = parse_qmd_virtual(target) {
        if let Ok(b) = conn.query_row(
            "SELECT doc FROM content JOIN documents d ON d.hash=content.hash WHERE d.collection=? AND d.path=? AND d.active=1",
            [&coll, &pth],
            |r| r.get(0),
        ) {
            return Some(b);
        }
        if let Ok(b) = conn.query_row(
            "SELECT doc FROM content JOIN documents d ON d.hash=content.hash WHERE d.collection=? AND d.path LIKE ? ESCAPE '\\' AND d.active=1 LIMIT 1",
            [&coll, &format!("%{}", escape_like(&pth))],
            |r| r.get(0),
        ) {
            return Some(b);
        }
    }
    // bare collection/path form
    if !target.starts_with('/') && !target.starts_with('~') && target.contains('/') {
        let mut it = target.splitn(2, '/');
        if let (Some(coll), Some(pth)) = (it.next(), it.next()) {
            if let Ok(b) = conn.query_row(
                "SELECT doc FROM content JOIN documents d ON d.hash=content.hash WHERE d.collection=? AND d.path=? AND d.active=1",
                [coll, pth],
                |r| r.get(0),
            ) {
                return Some(b);
            }
            if let Ok(b) = conn.query_row(
                "SELECT doc FROM content JOIN documents d ON d.hash=content.hash WHERE d.collection=? AND d.path LIKE ? ESCAPE '\\' AND d.active=1 LIMIT 1",
                [coll, &format!("%{}", escape_like(pth))],
                |r| r.get(0),
            ) {
                return Some(b);
            }
        }
    }
    None
}
