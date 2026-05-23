//! Full-text search (BM25 via SQLite FTS5).
//!
//! This module contains the query sanitization and FTS5 builder ported from the
//! original TypeScript implementation (src/store.ts) for behavioral parity.

use anyhow::Result;
use rusqlite::Connection;

use super::{expand_tilde, open_connection};

/// Result of an FTS5 search.
#[derive(Debug, Clone)]
pub struct FtsHit {
    pub file: String,
    pub docid: String,
    pub title: String,
    pub score: f32,
    pub snippet: String,
}

// --- CJK and sanitization helpers (faithful port) ---

pub fn is_cjk_char(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0x1100..=0x11FF).contains(&cp)
        || (0x3130..=0x318F).contains(&cp)
}

pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(is_cjk_char)
}

pub fn normalize_cjk_for_fts(text: &str) -> String {
    let mut out = String::new();
    let cs: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if is_cjk_char(cs[i]) {
            out.push(' ');
            while i < cs.len() && is_cjk_char(cs[i]) {
                out.push(cs[i]);
                out.push(' ');
                i += 1;
            }
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    out
}

pub fn sanitize_fts5_term(term: &str) -> String {
    // Very simplified version of the original logic for now.
    // The full port is quite long; this keeps the build working.
    term.replace(|c: char| !c.is_alphanumeric() && c != '\'', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn is_hyphenated_token(term: &str) -> bool {
    let parts: Vec<&str> = term.split('-').collect();
    parts.len() > 1 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '\''))
}

pub fn sanitize_hyphenated_term(term: &str) -> String {
    term.replace('-', " ")
}

pub fn sanitize_fts5_phrase(phrase: &str) -> String {
    format!("\"{}\"", phrase.replace('"', ""))
}

pub fn build_fts5_query(query: &str) -> Option<String> {
    // For the initial extraction we keep a working (if slightly simplified) version.
    // The full faithful port from the TS code can be moved here later.
    if query.trim().is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    for token in query.split_whitespace() {
        if token.starts_with('-') {
            let t = token.trim_start_matches('-');
            if !t.is_empty() {
                parts.push(format!("NOT {}", sanitize_fts5_term(t)));
            }
        } else if token.starts_with('"') && token.ends_with('"') {
            parts.push(sanitize_fts5_phrase(&token[1..token.len()-1]));
        } else if is_hyphenated_token(token) {
            parts.push(sanitize_hyphenated_term(token));
        } else {
            parts.push(sanitize_fts5_term(token));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" AND "))
    }
}

/// Perform an FTS5 BM25 search.
pub fn fts_search(query: &str, limit: usize, collection: Option<&str>) -> Result<Vec<FtsHit>> {
    let fts_query = match build_fts5_query(query) {
        Some(q) => q,
        None => return Ok(vec![]),
    };

    let conn = open_connection(true)?;

    let mut sql = String::from(
        r#"
        SELECT d.path, d.hash, c.doc, bm25(documents_fts) as score
        FROM documents_fts
        JOIN documents d ON d.id = documents_fts.rowid
        JOIN content c ON c.hash = d.hash
        WHERE documents_fts MATCH ?
        "#,
    );

    if collection.is_some() {
        sql.push_str(" AND d.collection = ?");
    }

    sql.push_str(" ORDER BY score LIMIT ?");

    let mut stmt = conn.prepare(&sql)?;

    let mut hits = Vec::new();

    if let Some(coll) = collection {
        let mut rows = stmt.query((&fts_query, coll, limit as i64))?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let hash: String = row.get(1)?;
            let doc: String = row.get(2)?;
            let score: f64 = row.get(3)?;

            let title = doc.lines().next().unwrap_or(&path).to_string();
            let snippet = doc.chars().take(220).collect();

            hits.push(FtsHit {
                file: path,
                docid: hash.chars().take(6).collect(),
                title,
                score: (1.0 - (score as f32 / 100.0)).clamp(0.0, 1.0),
                snippet,
            });
        }
    } else {
        let mut rows = stmt.query((&fts_query, limit as i64))?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let hash: String = row.get(1)?;
            let doc: String = row.get(2)?;
            let score: f64 = row.get(3)?;

            let title = doc.lines().next().unwrap_or(&path).to_string();
            let snippet = doc.chars().take(220).collect();

            hits.push(FtsHit {
                file: path,
                docid: hash.chars().take(6).collect(),
                title,
                score: (1.0 - (score as f32 / 100.0)).clamp(0.0, 1.0),
                snippet,
            });
        }
    }

    Ok(hits)
}
