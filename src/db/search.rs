//! Full-text search (BM25 via SQLite FTS5) — faithful port of the original logic.

use anyhow::Result;

use super::open_connection;

#[derive(serde::Serialize, Debug, Clone)]
pub struct FtsHit {
    pub file: String,
    pub docid: String,
    pub title: String,
    pub score: f32,
    pub snippet: String,
}

// === CJK & sanitization helpers (exact semantics from original store.ts) ===

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
            out.push(' ');
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    out
}

pub fn sanitize_fts5_term(term: &str) -> String {
    term.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '_')
        .collect::<String>()
        .to_lowercase()
}

pub fn is_hyphenated_token(term: &str) -> bool {
    if !term.contains('-') || term.starts_with('-') || term.ends_with('-') {
        return false;
    }
    let parts: Vec<&str> = term.split('-').collect();
    let alnum_parts = parts
        .iter()
        .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '\''))
        .count();
    alnum_parts >= 2
}

pub fn sanitize_hyphenated_term(term: &str) -> String {
    term.split('-')
        .map(sanitize_fts5_term)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn sanitize_fts5_phrase(phrase: &str) -> String {
    normalize_cjk_for_fts(phrase)
        .split_whitespace()
        .map(sanitize_fts5_term)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn build_fts5_query(query: &str) -> Option<String> {
    let mut positive: Vec<String> = Vec::new();
    let mut negative: Vec<String> = Vec::new();
    let s = query.trim();
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        while i < cs.len() && cs[i].is_whitespace() {
            i += 1;
        }
        if i >= cs.len() {
            break;
        }
        let negated = cs[i] == '-';
        if negated {
            i += 1;
        }
        if i >= cs.len() {
            break;
        }
        if cs[i] == '"' {
            i += 1;
            let start = i;
            while i < cs.len() && cs[i] != '"' {
                i += 1;
            }
            let phrase: String = cs[start..i].iter().collect();
            if i < cs.len() {
                i += 1;
            }
            let sanitized = sanitize_fts5_phrase(&phrase);
            if !sanitized.is_empty() {
                let fts = format!("\"{}\"", sanitized);
                if negated {
                    negative.push(fts);
                } else {
                    positive.push(fts);
                }
            }
        } else {
            let start = i;
            while i < cs.len() && !(cs[i].is_whitespace() || cs[i] == '"') {
                i += 1;
            }
            let term: String = cs[start..i].iter().collect();
            if is_hyphenated_token(&term) {
                let sanitized = sanitize_hyphenated_term(&term);
                if !sanitized.is_empty() {
                    let fts = format!("\"{}\"", sanitized);
                    if negated {
                        negative.push(fts);
                    } else {
                        positive.push(fts);
                    }
                }
            } else if contains_cjk(&term) {
                let sanitized = sanitize_fts5_phrase(&term);
                if !sanitized.is_empty() {
                    let fts = format!("\"{}\"", sanitized);
                    if negated {
                        negative.push(fts);
                    } else {
                        positive.push(fts);
                    }
                }
            } else {
                let sanitized = sanitize_fts5_term(&term);
                if !sanitized.is_empty() {
                    let fts = format!("\"{}\"*", sanitized);
                    if negated {
                        negative.push(fts);
                    } else {
                        positive.push(fts);
                    }
                }
            }
        }
    }
    if positive.is_empty() {
        return None;
    }
    let mut res = positive.join(" AND ");
    for neg in negative {
        res = format!("{} NOT {}", res, neg);
    }
    Some(res)
}

pub fn fts_search(query: &str, limit: usize, collection: Option<&str>) -> Result<Vec<FtsHit>> {
    let fts_q = match build_fts5_query(query) {
        Some(q) => q,
        None => return Ok(vec![]),
    };
    let conn = open_connection(true)?;
    let limit_i = limit as i64;

    if let Some(c) = collection {
        let fts_limit = (limit * 10) as i64;
        let sql = r#"
            WITH fts_matches AS (
              SELECT rowid, bm25(documents_fts, 1.5, 4.0, 1.0) as sc
              FROM documents_fts
              WHERE documents_fts MATCH ?
              ORDER BY sc ASC
              LIMIT ?
            )
            SELECT
              'qmd://' || d.collection || '/' || d.path as filepath,
              d.title, d.hash, content.doc as body, fm.sc
            FROM fts_matches fm
            JOIN documents d ON d.id = fm.rowid
            JOIN content ON content.hash = d.hash
            WHERE d.active = 1 AND d.collection = ?
            ORDER BY fm.sc ASC LIMIT ?
        "#;
        let mut stmt = conn.prepare(sql)?;
        let rows_iter = stmt.query_map(rusqlite::params![fts_q, fts_limit, c, limit_i], |r| {
            let filepath: String = r.get(0)?;
            let title: String = r.get(1)?;
            let hash: String = r.get(2)?;
            let body: String = r.get(3)?;
            let sc: f64 = r.get(4)?;
            let score = (sc.abs() / (1.0 + sc.abs())) as f32;
            let docid = if hash.len() >= 6 {
                format!("#{}", &hash[0..6])
            } else {
                format!("#{}", hash)
            };
            let snippet: String = body.chars().take(220).collect();
            Ok(FtsHit {
                file: filepath,
                docid,
                title,
                score,
                snippet,
            })
        })?;
        let rows: Vec<FtsHit> = rows_iter.filter_map(|x| x.ok()).collect();
        Ok(rows)
    } else {
        let sql = "SELECT 'qmd://' || d.collection || '/' || d.path as filepath, d.title, d.hash, content.doc as body, bm25(documents_fts, 1.5, 4.0, 1.0) as sc FROM documents_fts JOIN documents d ON d.id = documents_fts.rowid JOIN content ON content.hash = d.hash WHERE documents_fts MATCH ? AND d.active = 1 ORDER BY sc ASC LIMIT ?";
        let mut stmt = conn.prepare(sql)?;
        let rows_iter = stmt.query_map(rusqlite::params![fts_q, limit_i], |r| {
            let filepath: String = r.get(0)?;
            let title: String = r.get(1)?;
            let hash: String = r.get(2)?;
            let body: String = r.get(3)?;
            let sc: f64 = r.get(4)?;
            let score = (sc.abs() / (1.0 + sc.abs())) as f32;
            let docid = if hash.len() >= 6 {
                format!("#{}", &hash[0..6])
            } else {
                format!("#{}", hash)
            };
            let snippet: String = body.chars().take(220).collect();
            Ok(FtsHit {
                file: filepath,
                docid,
                title,
                score,
                snippet,
            })
        })?;
        let rows: Vec<FtsHit> = rows_iter.filter_map(|x| x.ok()).collect();
        Ok(rows)
    }
}
