//! Minimal `qmd bench <fixture.json>` implementation (Iteration 1).
//!
//! Loads a simple JSON fixture (queries + expected_files + optional expected_in_top_k).
//! Exercises the existing FTS search path (core of `search` / `query` lex).
//! Reports recall (hit rate within k), average latency, and basic stats.
//! No new dependencies; uses only serde_json (already in tree) + std.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Instant;

use crate::db::search::fts_search;

#[derive(Debug, Deserialize)]
struct Fixture {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    collection: Option<String>,
    queries: Vec<QueryCase>,
}

#[derive(Debug, Deserialize)]
struct QueryCase {
    id: String,
    query: String,
    #[serde(default)]
    expected_files: Vec<String>,
    #[serde(default = "default_k")]
    expected_in_top_k: usize,
}

fn default_k() -> usize {
    5
}

pub fn cmd_bench(fixture: PathBuf) -> Result<()> {
    let text = std::fs::read_to_string(&fixture)
        .with_context(|| format!("failed to read fixture: {}", fixture.display()))?;
    let f: Fixture = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse fixture JSON: {}", fixture.display()))?;

    println!(
        "Bench: {} queries (collection filter: {})",
        f.queries.len(),
        f.collection.as_deref().unwrap_or("<any>")
    );
    if let Some(d) = &f.description {
        println!("  {}", d);
    }
    println!();

    let mut hit_count = 0usize;
    let mut latencies_ms: Vec<u128> = Vec::with_capacity(f.queries.len());
    let coll = f.collection.as_deref();

    for case in &f.queries {
        let t0 = Instant::now();
        let hits = fts_search(&case.query, 50, coll)?;
        let dt = t0.elapsed();
        latencies_ms.push(dt.as_millis());

        let k = case.expected_in_top_k;
        let top_files: Vec<&str> = hits.iter().take(k).map(|h| h.file.as_str()).collect();

        let matched = case.expected_files.iter().any(|exp| {
            top_files
                .iter()
                .any(|f| f.contains(exp) || f.ends_with(exp))
        });

        if matched {
            hit_count += 1;
        }

        println!(
            "  [{}] q={} k={} hit={} ({} hits)",
            case.id,
            case.query,
            k,
            if matched { "yes" } else { "no" },
            hits.len()
        );
    }

    let n = f.queries.len() as f64;
    let recall = if n > 0.0 { hit_count as f64 / n } else { 0.0 };
    let avg_lat = if !latencies_ms.is_empty() {
        latencies_ms.iter().sum::<u128>() as f64 / latencies_ms.len() as f64
    } else {
        0.0
    };

    println!();
    println!("=== Bench summary ===");
    println!("Queries:   {}", f.queries.len());
    println!(
        "Recall@K:  {:.2} ({} / {})",
        recall,
        hit_count,
        f.queries.len()
    );
    println!("Avg latency: {:.1} ms", avg_lat);
    if !latencies_ms.is_empty() {
        let min = *latencies_ms.iter().min().unwrap();
        let max = *latencies_ms.iter().max().unwrap();
        println!("Latency range: {}..{} ms", min, max);
    }
    Ok(())
}
