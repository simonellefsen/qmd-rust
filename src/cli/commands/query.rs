//! Implementation of `qmd query` and `qmd vsearch` (Area 1 + Area 2 + 0.5 finish).
//!
//! - lex: / Simple + intent: → FTS5 (intent augments for real expansion)
//! - vec: / hyde: + hybrid → real cosine (embedder) + RRF
//! - --no_rerank respected; real rerank is graceful stub (future behind embed feature)
//! - vsearch real when embedder present. Smallest diff from prior, full flag parity.

use super::{ClauseKind, ParsedQuery};
use crate::cli::args::OutputFormat;
use crate::db::search as db_search;
use crate::embed::{default_embedder, Embedder};
use anyhow::Result;

/// Handle `qmd query ...` — now with real expansion (intent augments lex search text)
/// and hyde/vec clauses feeding the vector path when available. Rerank flag is wired
/// (real cross-encoder behind embed feature is future; graceful no-op here).
///
/// For Rust newbies: the structured parser (from Area 1) already extracts intent:/lex:/vec:/hyde:.
/// We now *use* intent and hyde for actual retrieval (smallest expansion without a
/// separate LLM generate call). The no_rerank bool comes straight from clap; we
/// respect it for explain output and future rerank hook. All other flags (min_score,
/// collection, explain, full, etc.) were already respected — kept exact.
#[allow(clippy::too_many_arguments)]
pub fn cmd_query(
    query: Vec<String>,
    n: usize,
    all: bool,
    min_score: Option<f32>,
    format: OutputFormat,
    collection: Option<String>,
    explain: bool,
    no_rerank: bool,
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
                eprintln!("explain: structured query");
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

    // Resolve embedder once (cheap when noop; real load happens on first embed_batch).
    let embedder: Box<dyn Embedder> = default_embedder();
    let can_vec = embedder.dimension() > 0;

    let (search_text, display_for_empty, vec_clause_text) = match &parsed {
        ParsedQuery::Simple(text) => {
            let s = text.clone();
            (s.clone(), s, None)
        }
        ParsedQuery::Structured {
            intent, clauses, ..
        } => {
            let lex: Vec<&str> = clauses
                .iter()
                .filter(|c| c.kind == ClauseKind::Lex)
                .map(|c| c.text.as_str())
                .collect();
            // vec: + hyde: both feed vector search (hyde provides the "generated" hypothetical for embedding)
            let vec_clauses: Vec<&str> = clauses
                .iter()
                .filter(|c| c.kind == ClauseKind::Vec || c.kind == ClauseKind::Hyde)
                .map(|c| c.text.as_str())
                .collect();
            let non_lex = clauses.iter().filter(|c| c.kind != ClauseKind::Lex).count();
            if non_lex > 0 && !can_vec {
                eprintln!("Vector/HyDE search requires embeddings (build with `llama-embed` feature + set QMD_EMBED_MODEL).");
            }
            let vtext = if !vec_clauses.is_empty() {
                Some(vec_clauses[0].to_string())
            } else {
                None
            };
            if lex.is_empty() && vtext.is_none() {
                return Ok(());
            }
            let joined_lex = lex.join(" ");
            // Real intent expansion (smallest form, no extra LLM roundtrip): intent text
            // augments the lex search so FTS5 sees both the high-level goal and the keywords.
            let search = if let Some(i) = intent {
                if joined_lex.trim().is_empty() {
                    i.clone()
                } else {
                    format!("{} {}", i, joined_lex)
                }
            } else {
                joined_lex
            };
            (search, input.clone(), vtext)
        }
    };

    let lim = if all { 500 } else { n };
    let coll = collection.as_deref();

    // Lex path (always available)
    let mut hits = if !search_text.trim().is_empty() {
        db_search::fts_search(&search_text, lim, coll)?
    } else {
        vec![]
    };

    // Vec path (when available and requested)
    if let Some(vtext) = &vec_clause_text {
        if can_vec {
            // Format like original for query embeddings (best-effort fidelity for this slice)
            let formatted = format!("task: search result | query: {}", vtext);
            match embedder.embed_batch(&[formatted.as_str()]) {
                Ok(vs) if !vs.is_empty() => {
                    let qv = &vs[0];
                    match db_search::vec_search(qv, lim, coll) {
                        Ok(vhits) => {
                            if hits.is_empty() {
                                hits = vhits;
                            } else if !vhits.is_empty() {
                                // Basic RRF fusion for hybrid lex + vec results (#3)
                                hits = fuse_rrf(hits, vhits);
                            }
                        }
                        Err(e) => eprintln!("  vec search failed: {}", e),
                    }
                }
                Ok(_) => {}
                Err(e) => eprintln!("  query embedding failed: {}", e),
            }
        }
    }

    // no_rerank is now wired (was ignored). Real reranker (qwen-style or llama-backed)
    // would be applied here on the fused candidate set when !no_rerank && can_vec.
    // Graceful degradation: we simply keep the current hybrid scores. This makes
    // `qmd query` the recommended command with the functionality that *is* present.
    if !no_rerank && explain {
        eprintln!("(rerank: real LLM reranker not wired in 0.5 slice; using fused lex+vec scores)");
    }

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

/// Handle `qmd vsearch ...` — now real (cosine over stored content_vectors) when embedder present.
/// Pure vector path; falls back to polite message only if no real embedder (dim==0).
#[allow(clippy::too_many_arguments)]
pub fn cmd_vsearch(
    query: Vec<String>,
    n: usize,
    all: bool,
    min_score: Option<f32>,
    format: OutputFormat,
    collection: Option<String>,
    full: bool,
    line_numbers: bool,
) -> Result<()> {
    let input = query.join(" ");
    if input.trim().is_empty() {
        eprintln!("vsearch: empty query");
        return Ok(());
    }

    let embedder: Box<dyn Embedder> = default_embedder();
    if embedder.dimension() == 0 {
        eprintln!("vsearch requires embeddings (build with `llama-embed` + QMD_EMBED_MODEL=/path/to/gguf).");
        eprintln!("Falling back to lex search is `qmd search` or `qmd query`.");
        return Ok(());
    }

    // Format query text for embedding (mirrors original behavior for the active model)
    let formatted = format!("task: search result | query: {}", input);
    let vecs = match embedder.embed_batch(&[formatted.as_str()]) {
        Ok(v) if !v.is_empty() => v,
        Ok(_) => {
            eprintln!("vsearch: embedder produced no vector");
            return Ok(());
        }
        Err(e) => {
            eprintln!("vsearch: embedding failed: {}", e);
            return Ok(());
        }
    };

    let qv = &vecs[0];
    let lim = if all { 500 } else { n };
    let coll = collection.as_deref();
    let mut hits = match db_search::vec_search(qv, lim, coll) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("vsearch: {}", e);
            vec![]
        }
    };

    if let Some(ms) = min_score {
        hits.retain(|h| h.score >= ms);
    }

    // Same output formatting as cmd_query (keeps surface parity, smallest diff)
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
            if hits.is_empty() {
                println!("No vector matches for '{}'", input);
            } else {
                for h in &hits {
                    println!("{} {}", h.file, h.docid);
                    println!("Title: {}", h.title);
                    println!("Score: {:.3} (cosine)", h.score);
                    println!();

                    if full {
                        let content = if line_numbers {
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

/// Very small RRF fusion for hybrid lex + vec results in `query`.
/// k=60 is a common default; keeps top docs from either list.
fn fuse_rrf(lex: Vec<db_search::FtsHit>, vecs: Vec<db_search::FtsHit>) -> Vec<db_search::FtsHit> {
    use std::collections::HashMap;
    let mut score: HashMap<String, f32> = HashMap::new();
    let mut meta: HashMap<String, db_search::FtsHit> = HashMap::new();

    for (rank, h) in lex.into_iter().enumerate() {
        let s = score.entry(h.file.clone()).or_insert(0.0);
        *s += 1.0 / (60.0 + rank as f32);
        meta.entry(h.file.clone()).or_insert(h);
    }
    for (rank, h) in vecs.into_iter().enumerate() {
        let s = score.entry(h.file.clone()).or_insert(0.0);
        *s += 1.0 / (60.0 + rank as f32);
        meta.entry(h.file.clone()).or_insert(h);
    }

    let mut out: Vec<_> = meta
        .into_iter()
        .map(|(f, mut m)| {
            if let Some(&sc) = score.get(&f) {
                m.score = sc;
            }
            m
        })
        .collect();

    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}
