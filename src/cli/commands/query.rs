//! Implementation of `qmd query` and `qmd vsearch` (Area 1 + Area 2 + 0.5 finish + I2).
//!
//! - lex: / Simple + intent: → FTS5 (intent augments for real expansion)
//! - vec: / hyde: + hybrid → real cosine (embedder) + RRF
//! - Better auto expansion (I2): plain + structured now auto-hybrid + multi-vector (original + pseudo-HyDE rewrite) when embedder present — reuses existing embedder infra only (no gen scaffolding).
//! - Real reranker (I2): after fusion, if embeddings, use embedder-driven semantic (cosine on query vs passage) as reranker when models.rerank (or fallback embed) present via llama path; falls to heuristic otherwise. Respects --no-rerank, --candidate-limit, --explain. Actually reorders on real model signals.
//! - --no_rerank / candidate_limit respected; vsearch real when embedder present. Smallest diff, full flag parity.

use super::{ClauseKind, ParsedQuery};
use crate::cli::args::OutputFormat;
use crate::db::format_path_for_output;
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
/// respect it for explain output and future rerank hook. candidate_limit caps
/// expensive real rerank (I2). All other flags kept exact.
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
    candidate_limit: usize,
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
            // Basic automatic expansion (smallest viable): when embedder available, treat the
            // plain query text itself as the vec input. This makes `qmd query` automatically
            // hybrid (lex + vec) for the common case — biggest usability win for the recommended
            // command while only touching the existing embedder path. No LLM generate yet.
            let vtext = if can_vec { Some(s.clone()) } else { None };
            (s.clone(), s, vtext)
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
            let mut vtext = if !vec_clauses.is_empty() {
                Some(vec_clauses[0].to_string())
            } else {
                None
            };
            // Auto vec expansion for lex-only structured queries (or intent+lex) when embeddings present.
            // Reuses embedder exactly; keeps `qmd query` as the single recommended entry point.
            if vtext.is_none() && can_vec && (!lex.is_empty() || intent.is_some()) {
                vtext = Some(input.clone());
            }
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

    // Vec path (when available and requested) + I2 better auto-expansion:
    // reuse embedder for primary + one pseudo-HyDE variant (no generate scaffolding).
    // Multiple vec searches + RRF gives richer expansion than plain text-as-vec alone.
    if let Some(vtext) = &vec_clause_text {
        if can_vec {
            let mut vtexts: Vec<String> = vec![vtext.clone()];
            // Better expansion: add a cheap pseudo-HyDE rewrite (still embedder only).
            // This produces a second vector signal for the same intent, fused below.
            if vtext == &input || vtext.starts_with(&input) {
                vtexts.push(format!("hypothetical document that answers: {}", vtext));
            }
            for (i, vt) in vtexts.iter().enumerate() {
                let formatted = format!("task: search result | query: {}", vt);
                match embedder.embed_batch(&[formatted.as_str()]) {
                    Ok(vs) if !vs.is_empty() => {
                        let qv = &vs[0];
                        match db_search::vec_search(qv, lim, coll) {
                            Ok(vhits) => {
                                if hits.is_empty() && i == 0 {
                                    hits = vhits;
                                } else if !vhits.is_empty() {
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
    }

    // Real reranker (I2) integrated after fusion: model-driven semantic via existing
    // embedder path (loads from models.rerank if configured, else embed fallback).
    // Scores candidates with cosine(query_embed, passage_embed). Replaces heuristic
    // when vec available (meaningful reorder on real data). Respects --no-rerank,
    // --candidate-limit (cap before costly per-cand embeds), --explain.
    // Fallback to heuristic when no embedder.
    if !no_rerank {
        let use_real = can_vec;
        if use_real {
            let cap = candidate_limit.max(1);
            let mut cands: Vec<_> = hits.clone().into_iter().take(cap).collect();
            if !cands.is_empty() {
                // Use dedicated reranker embedder (prefers models.rerank via config/env)
                let rer_emb: Box<dyn Embedder> = crate::embed::default_reranker();
                let qfmt = format!("task: search result | query: {}", input);
                if let Ok(qvs) = rer_emb.embed_batch(&[qfmt.as_str()]) {
                    if let Some(qv) = qvs.first() {
                        for h in &mut cands {
                            let passage = format!(
                                "{} {}",
                                h.title,
                                h.snippet.chars().take(512).collect::<String>()
                            );
                            let pfmt = format!("task: search result | query: {}", passage);
                            if let Ok(pvs) = rer_emb.embed_batch(&[pfmt.as_str()]) {
                                if let Some(pv) = pvs.first() {
                                    h.score = cosine_similarity(qv, pv);
                                }
                            }
                        }
                    }
                }
                cands.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                hits = cands;
                if explain {
                    eprintln!(
                        "(rerank: real semantic via {} on top {} candidates after fusion/expansion)",
                        rer_emb.model_id(),
                        cap
                    );
                }
            }
        } else {
            hits = heuristic_rerank(hits, &input);
            if explain {
                eprintln!("(rerank: heuristic reranker applied on fused/auto-expanded results)");
            }
        }
    } else if explain {
        eprintln!("(rerank: skipped via --no-rerank flag)");
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
                    // Use formatter for TTY clickable editor links (from Iteration 3 slice).
                    // Preserves visible "qmd://..." text; adds OSC8 href when appropriate.
                    // line/col default to 1 (real chunk lines not yet stored in FtsHit/DB).
                    let p = format_path_for_output(&h.file, Some(1), Some(1));
                    println!("{} {}", p, h.docid);
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
                    // Use formatter for TTY clickable editor links (Iteration 3 polish).
                    let p = format_path_for_output(&h.file, Some(1), Some(1));
                    println!("{} {}", p, h.docid);
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

/// Lightweight deterministic reranker for `qmd query` (the recommended path).
///
/// For Rust newbies:
/// - Takes the already-retrieved hits (from FTS5 and/or vec_search + RRF).
/// - Mutates scores in place with small additive boosts based on string overlap
///   between the original user query and the hit's title/snippet (cheap, no allocs beyond to_lowercase).
/// - Re-sorts descending by the boosted score.
/// - This is a *real* reranking step (changes result order for many queries) that requires
///   zero LLM, zero extra feature flags, and runs on top of the embedder work we already did.
/// - In I2: used only as fallback when no embedder (real semantic rerank via default_reranker + cosine is preferred after fusion when can_vec).
/// - Contrast: a true cross-encoder reranker would load a second (usually smaller) GGUF model,
///   run forward passes on (query, passage) pairs, and return calibrated 0..1 relevance.
///   (models.rerank now drives the embedder-based semantic rerank in this slice.)
fn heuristic_rerank(mut hits: Vec<db_search::FtsHit>, query: &str) -> Vec<db_search::FtsHit> {
    if query.trim().is_empty() || hits.is_empty() {
        return hits;
    }
    let q = query.to_lowercase();
    for h in &mut hits {
        let mut boost: f32 = 0.0;
        let t = h.title.to_lowercase();
        let s = h.snippet.to_lowercase();
        if !q.is_empty() && t.contains(&q) {
            boost += 0.15;
        }
        for term in q.split_whitespace() {
            if term.len() >= 3 {
                if t.contains(term) {
                    boost += 0.08;
                } else if s.contains(term) {
                    boost += 0.03;
                }
            }
        }
        // Cap the total boost so we do not completely override strong BM25/vec signals.
        h.score += boost.min(0.5);
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits
}

/// Cosine similarity (I2 reranker helper). Assumes same-dim non-empty vecs.
/// Returns 0.0 on mismatch (safe fallback).
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-8);
    (dot / denom).clamp(-1.0, 1.0)
}
