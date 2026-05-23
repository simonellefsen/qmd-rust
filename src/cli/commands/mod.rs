//! Individual command implementations (cmd_status, cmd_search, cmd_get, etc.)
//!
//! Each command lives in its own file for maintainability.
//! Shared path/FS helpers for ls/get/mcp live here (pub(crate) so submodules can `use super::` them).

pub mod collection;
pub mod get;
pub mod ls;
pub mod mcp;
pub mod query;
pub mod search;
pub mod status;

use crate::db::open_connection;

use anyhow::{bail, Result};

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

/// Query clause kinds for structured `query` documents (lex-only path for v0.3.0 slice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClauseKind {
    Lex,
    Vec,
    Hyde,
}

/// One clause in a structured query document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryClause {
    pub kind: ClauseKind,
    pub text: String,
}

/// Result of `parse_structured_query`: either a plain/simple query (treated as lex here)
/// or a structured document with optional intent + typed clauses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedQuery {
    /// Plain text (single line, no prefix) or `expand: ...` — treat as lex search in this slice.
    Simple(String),
    /// Multi-line with `lex:`, `vec:`, `hyde:`, optional `intent:`.
    Structured {
        intent: Option<String>,
        clauses: Vec<QueryClause>,
    },
}

/// Parse a query string (supports $'multi\nline' or single) following the grammar in docs/SYNTAX.md
/// and the exact logic from original-ts/src/cli/qmd.ts:parseStructuredQuery (lines ~2376-2444).
///
/// - Single plain line or single `expand:` line → ParsedQuery::Simple(text)
/// - Structured with lex/vec/hyde (+ at most one intent:) → Structured
/// - Strict validation: no mixing plain+typed, at most one intent, no lone intent, no newlines inside clause text.
/// - Negation/phrases/wildcards stay in the `text` and are passed through to FTS5 `build_fts5_query`.
///
/// This is the first real code for Area 1 / 0.3.0 (lex path only; vec/hyde graceful).
pub fn parse_structured_query(input: &str) -> Result<ParsedQuery> {
    let raw_lines: Vec<String> = input
        .split('\n')
        .map(|l| l.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    if raw_lines.is_empty() {
        return Ok(ParsedQuery::Simple(String::new()));
    }

    let mut intent: Option<String> = None;
    let mut clauses: Vec<QueryClause> = Vec::new();

    for (idx, trimmed) in raw_lines.iter().enumerate() {
        let line_num = idx + 1;
        let lower = trimmed.to_lowercase();

        // expand: (single only; treat as Simple for this lex slice)
        if lower.starts_with("expand:") {
            if raw_lines.len() > 1 {
                bail!(
                    "Line {} starts with expand:, but query documents cannot mix expand with typed lines. Submit a single expand query instead.",
                    line_num
                );
            }
            let text = trimmed[7..].trim().to_string();
            if text.is_empty() {
                bail!("expand: query must include text.");
            }
            return Ok(ParsedQuery::Simple(text));
        }

        // intent:
        if lower.starts_with("intent:") {
            if intent.is_some() {
                bail!(
                    "Line {}: only one intent: line is allowed per query document.",
                    line_num
                );
            }
            let text = trimmed[7..].trim().to_string();
            if text.is_empty() {
                bail!("Line {}: intent: must include text.", line_num);
            }
            intent = Some(text);
            continue;
        }

        // typed clauses
        let (kind, text) = if lower.starts_with("lex:") {
            let t = trimmed[4..].trim().to_string();
            (ClauseKind::Lex, t)
        } else if lower.starts_with("vec:") {
            let t = trimmed[4..].trim().to_string();
            (ClauseKind::Vec, t)
        } else if lower.starts_with("hyde:") {
            let t = trimmed[5..].trim().to_string();
            (ClauseKind::Hyde, t)
        } else {
            // plain line in multi-line doc
            if raw_lines.len() == 1 {
                return Ok(ParsedQuery::Simple(trimmed.clone()));
            }
            bail!(
                "Line {} is missing a lex:/vec:/hyde:/intent: prefix. Each line in a query document must start with one.",
                line_num
            );
        };

        if text.is_empty() {
            let label = match kind {
                ClauseKind::Lex => "lex",
                ClauseKind::Vec => "vec",
                ClauseKind::Hyde => "hyde",
            };
            bail!("Line {} ({}:) must include text.", line_num, label);
        }
        // Retained for exact fidelity with TS parseStructuredQuery (which performs the identical post-extract check).
        // split('\n') + trim() make it unreachable for well-formed input, but we keep the guard + exact error
        // text to protect against future line-splitting refactors and to match the reference parser behavior.
        if text.contains('\n') || text.contains('\r') {
            let label = match kind {
                ClauseKind::Lex => "lex",
                ClauseKind::Vec => "vec",
                ClauseKind::Hyde => "hyde",
            };
            bail!(
                "Line {} ({}:) contains a newline. Keep each query on a single line.",
                line_num,
                label
            );
        }

        clauses.push(QueryClause { kind, text });
    }

    if intent.is_some() && clauses.is_empty() {
        bail!("intent: cannot appear alone. Add at least one lex:, vec:, or hyde: line.");
    }

    // All empty-clauses cases (single plain, single expand, lone intent) are handled
    // by early returns or bails above; reaching here means we have at least one clause.
    Ok(ParsedQuery::Structured { intent, clauses })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_plain() {
        assert_eq!(
            parse_structured_query("how does auth work").unwrap(),
            ParsedQuery::Simple("how does auth work".to_string())
        );
    }

    #[test]
    fn test_parse_simple_expand() {
        assert_eq!(
            parse_structured_query("expand: error handling").unwrap(),
            ParsedQuery::Simple("error handling".to_string())
        );
    }

    #[test]
    fn test_parse_lex_single() {
        let p = parse_structured_query("lex: CAP theorem consistency").unwrap();
        match p {
            ParsedQuery::Structured { clauses, .. } => {
                assert_eq!(clauses.len(), 1);
                assert_eq!(clauses[0].kind, ClauseKind::Lex);
                assert_eq!(clauses[0].text, "CAP theorem consistency");
            }
            _ => panic!("expected structured"),
        }
    }

    #[test]
    fn test_parse_lex_with_phrase_negation() {
        let p = parse_structured_query(r#"lex: "machine learning" -"deep learning""#).unwrap();
        match p {
            ParsedQuery::Structured { clauses, .. } => {
                assert_eq!(clauses[0].text, r#""machine learning" -"deep learning""#);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_multi_lex() {
        let p = parse_structured_query("lex: auth -oauth\nlex: token").unwrap();
        match p {
            ParsedQuery::Structured { clauses, .. } => {
                assert_eq!(clauses.len(), 2);
                assert!(clauses.iter().all(|c| c.kind == ClauseKind::Lex));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_with_intent() {
        let p = parse_structured_query(
            "intent: web performance\nlex: performance\nvec: how to improve",
        )
        .unwrap();
        match p {
            ParsedQuery::Structured { intent, clauses } => {
                assert_eq!(intent, Some("web performance".to_string()));
                assert_eq!(clauses.len(), 2);
                assert_eq!(clauses[0].kind, ClauseKind::Lex);
                assert_eq!(clauses[1].kind, ClauseKind::Vec);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_intent_alone_errors() {
        assert!(parse_structured_query("intent: foo").is_err());
    }

    #[test]
    fn test_parse_plain_in_multi_errors() {
        assert!(parse_structured_query("lex: foo\nbar").is_err());
    }

    #[test]
    fn test_parse_multi_intent_errors() {
        assert!(parse_structured_query("intent: a\nintent: b\nlex: x").is_err());
    }

    #[test]
    fn test_parse_expand_in_multi_errors() {
        assert!(parse_structured_query("expand: foo\nlex: bar").is_err());
    }

    #[test]
    fn test_parse_examples_from_syntax() {
        // from SYNTAX.md
        let _ = parse_structured_query("lex: CAP theorem consistency").unwrap();
        let _ = parse_structured_query(r#"lex: "machine learning" -"deep learning""#).unwrap();
        let _ = parse_structured_query("lex: auth -oauth -saml").unwrap();
        let _ =
            parse_structured_query("vec: how does the rate limiter handle burst traffic").unwrap();
        let p = parse_structured_query("intent: web page load times and Core Web Vitals\nlex: performance\nvec: how to improve performance").unwrap();
        if let ParsedQuery::Structured { intent, .. } = p {
            assert!(intent.is_some());
        }
    }

    #[test]
    fn test_parse_cjk_in_structured() {
        let p = parse_structured_query("lex: 日本語 検索\nvec: semantic japanese").unwrap();
        match p {
            ParsedQuery::Structured { clauses, .. } => {
                assert_eq!(clauses[0].text, "日本語 検索");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_empty_after_prefix_errors() {
        assert!(parse_structured_query("lex: ").is_err());
        assert!(parse_structured_query("lex:").is_err());
    }

    #[test]
    fn test_parse_only_vec_graceful_in_cmd() {
        // This exercises the graceful path in cmd_query (no lex clauses)
        // We can't easily unit test the full command without an index, but we can at least
        // ensure the parser produces the right shape so the graceful branch is taken.
        let p = parse_structured_query("vec: only vector here").unwrap();
        match p {
            ParsedQuery::Structured { clauses, .. } => {
                assert!(clauses.iter().all(|c| c.kind != ClauseKind::Lex));
            }
            _ => panic!(),
        }
    }
}
