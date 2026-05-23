//! Implementation of the `qmd search` command (BM25/FTS5 lexical search).
//!
//! Delegates query building and execution to `crate::db::search` (FTS5 sanitizers + CTEs).
//! Handles output formatting (text, json, files) and min_score filtering.

use crate::cli::args::OutputFormat;
use crate::db::search as db_search;
use anyhow::Result;

/// Handle `qmd search <query...>` with options.
#[allow(clippy::too_many_arguments)]
pub fn cmd_search(
    query: Vec<String>,
    n: usize,
    all: bool,
    min_score: Option<f32>,
    format: OutputFormat,
    collection: Option<String>,
    json: bool,
    files: bool,
) -> Result<()> {
    let joined = query.join(" ");
    if joined.trim().is_empty() {
        eprintln!("search: empty query");
        return Ok(());
    }
    let lim = if all { 500 } else { n };
    let coll = collection.as_deref();
    let mut hits = db_search::fts_search(&joined, lim, coll)?;
    if let Some(ms) = min_score {
        hits.retain(|h| h.score >= ms);
    }
    let effective = if json {
        OutputFormat::Json
    } else if files {
        OutputFormat::Files
    } else {
        format
    };
    match effective {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&hits)?);
        }
        OutputFormat::Files => {
            for h in &hits {
                println!("{}", h.file);
            }
        }
        _ => {
            if hits.is_empty() {
                println!("No matches for '{}'", joined);
            } else {
                for h in &hits {
                    println!("{} {}", h.file, h.docid);
                    println!("Title: {}", h.title);
                    println!("Score: {:.0}%", h.score * 100.0);
                    println!();
                    println!("{}", h.snippet);
                    println!();
                }
            }
        }
    }
    Ok(())
}
