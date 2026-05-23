//! Implementation of `qmd query` and `qmd vsearch` (first Area 1 slice for 0.3.0).
//!
//! - `parse_structured_query` lives in parent `mod.rs` (shared, minimal types).
//! - lex clauses (and Simple) delegate to the excellent existing `fts_search` + `build_fts5_query`.
//! - vec/hyde (and vsearch) emit the graceful "requires embeddings (Area 2 / v0.4.0)" message and exit 0.
//! - Follows exact patterns from search.rs / collection.rs etc. Smallest diff. No new deps.
//! - Supports all fields declared on Commands::Query / Vsearch today (explain prints parse; no_rerank ignored for lex path).

use super::{ClauseKind, ParsedQuery};
use crate::cli::args::OutputFormat;
use crate::db::search as db_search;
use anyhow::Result;

/// Handle `qmd query ...` — lex path only for this slice.
/// Simple text or `lex:` clauses → FTS5 via fts_search (reusing sanitizers, negation, phrases, CJK, collection filter, min_score, all/n).
/// Mixed or vec/hyde-only → polite message; lex parts still execute if present.
#[allow(clippy::too_many_arguments)]
pub fn cmd_query(
    query: Vec<String>,
    n: usize,
    all: bool,
    min_score: Option<f32>,
    format: OutputFormat,
    collection: Option<String>,
    explain: bool,
    _no_rerank: bool,
    full: bool,
    line_numbers: bool,
) -> Result<()> {
    let input = query.join(" ");
    if input.trim().is_empty() {
        eprintln!("query: empty query");
        return Ok(());
    }

    let parsed = super::parse_structured_query(&input)?;

    if explain {
        match &parsed {
            ParsedQuery::Simple(s) => {
                eprintln!("explain: simple lex query → {}", s);
            }
            ParsedQuery::Structured { intent, clauses } => {
                eprintln!("explain: structured query (lex-only path)");
                if let Some(i) = intent {
                    eprintln!("  intent: {}", i);
                }
                for c in clauses {
                    let k = match c.kind {
                        ClauseKind::Lex => "lex",
                        ClauseKind::Vec => "vec",
                        ClauseKind::Hyde => "hyde",
                    };
                    eprintln!("  {}: {}", k, c.text);
                }
            }
        }
    }

    let (search_text, display_for_empty) = match &parsed {
        ParsedQuery::Simple(text) => {
            let s = text.clone();
            (s.clone(), s)
        }
        ParsedQuery::Structured { clauses, .. } => {
            let lex: Vec<&str> = clauses
                .iter()
                .filter(|c| c.kind == ClauseKind::Lex)
                .map(|c| c.text.as_str())
                .collect();
            let non_lex = clauses.iter().filter(|c| c.kind != ClauseKind::Lex).count();
            if non_lex > 0 {
                eprintln!("Vector/HyDE search requires embeddings (coming in Area 2 / v0.4.0).");
            }
            if lex.is_empty() {
                return Ok(());
            }
            let joined_lex = lex.join(" ");
            (joined_lex, input.clone())
        }
    };

    let lim = if all { 500 } else { n };
    let coll = collection.as_deref();
    let mut hits = db_search::fts_search(&search_text, lim, coll)?;
    if let Some(ms) = min_score {
        hits.retain(|h| h.score >= ms);
    }

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&hits)?);
        }
        OutputFormat::Files => {
            for h in &hits {
                println!("{}", h.file);
            }
        }
        _ => {
            // Text (Csv/Md/Xml fall back to text for this slice)
            if hits.is_empty() {
                println!("No matches for '{}'", display_for_empty);
            } else {
                for h in &hits {
                    println!("{} {}", h.file, h.docid);
                    println!("Title: {}", h.title);
                    println!("Score: {:.0}%", h.score * 100.0);
                    println!();

                    if full {
                        // For full mode, we would ideally fetch the body.
                        // In this first slice we fall back to the snippet (or note).
                        // Proper body fetch can be added when we unify get logic.
                        let content = if line_numbers {
                            // naive numbering on the snippet we have
                            h.snippet
                                .lines()
                                .enumerate()
                                .map(|(i, l)| format!("{}: {}", i + 1, l))
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            h.snippet.clone()
                        };
                        println!("{}", content);
                    } else {
                        println!("{}", h.snippet);
                    }
                    println!();
                }
            }
        }
    }
    Ok(())
}

/// Handle `qmd vsearch ...` — graceful stub for this slice (vectors in 0.4.0).
/// Never crashes; always explains the status and suggests `search` or reference binary.
#[allow(clippy::too_many_arguments)]
pub fn cmd_vsearch(
    _query: Vec<String>,
    _n: usize,
    _all: bool,
    _min_score: Option<f32>,
    _format: OutputFormat,
    _collection: Option<String>,
    _full: bool,
    _line_numbers: bool,
) -> Result<()> {
    eprintln!("Vector/HyDE search requires embeddings (coming in Area 2 / v0.4.0).");
    eprintln!("Use `qmd search` (or `qmd query` with lex:) for keyword search today.");
    eprintln!("For full hybrid, use the reference: /opt/homebrew/bin/qmd vsearch ...");
    Ok(())
}
