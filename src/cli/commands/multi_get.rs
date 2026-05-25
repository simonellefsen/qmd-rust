//! Implementation of `qmd multi-get`.
//!
//! Batch document retrieval by glob pattern (e.g. "notes/2025*.md") or
//! comma-separated list of paths / qmd:// virtual paths.
//!
//! Supports -l/--max-bytes, all OutputFormat variants.
//! Follows the established per-command module pattern exactly
//! (see context.rs, cleanup.rs, get.rs).
//!
//! For Rust newbies:
//! - We open a read-only rusqlite connection (same as get/search).
//! - Comma vs glob detection mirrors the reference (no *,?,{ } => comma).
//! - Glob matching uses the already-declared `globset` crate (no new deps).
//! - Body size checks happen via LENGTH() before loading full content (efficient).
//! - All output paths (text/json/csv/etc.) are implemented for parity.
//! - Context inheritance is omitted in this smallest slice (YAML lookup + prefix
//!   logic lives in context.rs; pulling it here would bloat the diff).
//!
//! Never mutates state. Uses only temp dirs in any future tests (none added here).

use super::{escape_like, parse_qmd_virtual};
use crate::cli::args::OutputFormat;
use crate::db::open_connection;
use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rusqlite::params;
use std::path::Path;

/// Candidate metadata for a potential match (virtual path + length for prefilter).
#[derive(Debug, Clone)]
struct Candidate {
    virtual_path: String,
    body_len: usize,
    collection: String,
    path: String,
}

/// Final shaped result after size/line limiting.
#[derive(Debug, Clone)]
struct MultiResult {
    file: String,
    title: String,
    body: Option<String>,
    skipped: bool,
    skip_reason: Option<String>,
}

/// Detect comma-separated list vs glob (exact logic from reference behavior).
fn is_comma_list(pattern: &str) -> bool {
    pattern.contains(',')
        && !pattern.contains('*')
        && !pattern.contains('?')
        && !pattern.contains('{')
        && !pattern.contains('}')
}

/// Build a GlobSet for a user pattern. Supports **, *, ?, common cases.
fn build_globset(pattern: &str) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    // Try as written; globset handles ** on paths.
    if let Ok(g) = Glob::new(pattern) {
        b.add(g);
    } else {
        // Fallback: treat as simple suffix if it looks like a bare glob
        if let Ok(g) = Glob::new(&format!("**/{}", pattern.trim_start_matches('/'))) {
            b.add(g);
        }
    }
    b.build().context("invalid glob pattern")
}

/// Resolve a comma list into candidates (exact/suffix/virtual lookup, like get).
fn resolve_comma_list(conn: &rusqlite::Connection, pattern: &str) -> Result<Vec<Candidate>> {
    let mut cands = Vec::new();
    for raw in pattern
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if let Some(c) = resolve_single_name(conn, raw)? {
            cands.push(c);
        } else {
            eprintln!("File not found: {}", raw);
        }
    }
    Ok(cands)
}

/// Try to resolve one name (virtual, exact path, suffix path) into Candidate.
fn resolve_single_name(conn: &rusqlite::Connection, name: &str) -> Result<Option<Candidate>> {
    // Virtual path fast path
    if let Some((coll, pth)) = parse_qmd_virtual(name) {
        if let Ok((len, coll2, pth2)) = conn.query_row(
            "SELECT LENGTH(content.doc), d.collection, d.path
             FROM documents d JOIN content ON content.hash = d.hash
             WHERE d.collection = ? AND d.path = ? AND d.active=1
             LIMIT 1",
            params![coll, pth],
            |r| {
                Ok((
                    r.get::<_, i64>(0)? as usize,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        ) {
            let vpath = format!("qmd://{}/{}", coll2, pth2);
            return Ok(Some(Candidate {
                virtual_path: vpath,
                body_len: len,
                collection: coll2,
                path: pth2,
            }));
        }
    }

    // Bare path exact
    if let Ok((len, coll, pth)) = conn.query_row(
        "SELECT LENGTH(content.doc), d.collection, d.path
         FROM documents d JOIN content ON content.hash = d.hash
         WHERE d.path = ? AND d.active=1 LIMIT 1",
        params![name],
        |r| {
            Ok((
                r.get::<_, i64>(0)? as usize,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    ) {
        let vpath = format!("qmd://{}/{}", coll, pth);
        return Ok(Some(Candidate {
            virtual_path: vpath,
            body_len: len,
            collection: coll,
            path: pth,
        }));
    }

    // Suffix LIKE (mirrors get.rs + TS)
    let like = format!("%{}", escape_like(name));
    if let Ok((len, coll, pth)) = conn.query_row(
        "SELECT LENGTH(content.doc), d.collection, d.path
         FROM documents d JOIN content ON content.hash = d.hash
         WHERE d.path LIKE ? ESCAPE '\\' AND d.active=1 LIMIT 1",
        params![like],
        |r| {
            Ok((
                r.get::<_, i64>(0)? as usize,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    ) {
        let vpath = format!("qmd://{}/{}", coll, pth);
        return Ok(Some(Candidate {
            virtual_path: vpath,
            body_len: len,
            collection: coll,
            path: pth,
        }));
    }

    Ok(None)
}

/// Resolve glob by fetching lightweight metadata for all active docs, then
/// filtering in Rust with globset against the three common forms.
fn resolve_glob(conn: &rusqlite::Connection, pattern: &str) -> Result<Vec<Candidate>> {
    let gs = build_globset(pattern)?;

    let mut rows = conn.prepare(
        "SELECT
            'qmd://' || d.collection || '/' || d.path as virtual_path,
            d.path,
            d.collection,
            LENGTH(content.doc) as body_len
         FROM documents d
         JOIN content ON content.hash = d.hash
         WHERE d.active = 1",
    )?;

    let it = rows.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)? as usize,
        ))
    })?;

    let mut cands = Vec::new();
    for row in it {
        let (vpath, bare, coll, len) = row?;
        let coll_bare = format!("{}/{}", coll, bare);
        if gs.is_match(&vpath) || gs.is_match(&bare) || gs.is_match(&coll_bare) {
            cands.push(Candidate {
                virtual_path: vpath,
                body_len: len,
                collection: coll,
                path: bare,
            });
        }
    }

    if cands.is_empty() {
        eprintln!("No files matched pattern: {}", pattern);
    }
    Ok(cands)
}

/// Fetch body + title for a candidate, apply size and line limits, produce result.
fn fetch_and_limit(
    conn: &rusqlite::Connection,
    cand: Candidate,
    max_bytes: usize,
    max_lines: Option<usize>,
) -> Result<Option<MultiResult>> {
    if cand.body_len > max_bytes {
        let vpath = cand.virtual_path.clone();
        return Ok(Some(MultiResult {
            file: cand.virtual_path,
            title: Path::new(&cand.path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&cand.path)
                .to_string(),
            body: None,
            skipped: true,
            skip_reason: Some(format!(
                "File too large ({}KB > {}KB). Use 'qmd get {}' to retrieve.",
                cand.body_len.div_ceil(1024),
                max_bytes.div_ceil(1024),
                vpath
            )),
        }));
    }

    // Fetch actual body + title
    let (body, title): (String, Option<String>) = conn.query_row(
        "SELECT content.doc, d.title
         FROM documents d JOIN content ON content.hash = d.hash
         WHERE d.collection = ? AND d.path = ? AND d.active=1
         LIMIT 1",
        params![&cand.collection, &cand.path],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
    )?;

    let mut final_body = body;
    let final_title = title.unwrap_or_else(|| {
        Path::new(&cand.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&cand.path)
            .to_string()
    });

    // Apply line limit (append truncation note like reference)
    if let Some(ml) = max_lines {
        let lines: Vec<&str> = final_body.lines().collect();
        let n = lines.len();
        if n > ml {
            let kept: Vec<&str> = lines.into_iter().take(ml).collect();
            final_body = kept.join("\n");
            final_body.push_str(&format!("\n\n[... truncated {} more lines]", n - ml));
        }
    }

    Ok(Some(MultiResult {
        file: cand.virtual_path,
        title: final_title,
        body: Some(final_body),
        skipped: false,
        skip_reason: None,
    }))
}

/// Escape a field for CSV (double quotes).
fn escape_csv(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

/// Minimal XML escape.
fn escape_xml(val: &str) -> String {
    val.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Print results according to requested format (parity with reference).
fn output_results(results: Vec<MultiResult>, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            // Minimal shape: file, title, (body or skipped+reason). context omitted (smallest slice).
            let vals: Vec<serde_json::Value> = results
                .into_iter()
                .map(|r| {
                    if r.skipped {
                        serde_json::json!({
                            "file": r.file,
                            "title": r.title,
                            "skipped": true,
                            "reason": r.skip_reason.unwrap_or_default()
                        })
                    } else {
                        serde_json::json!({
                            "file": r.file,
                            "title": r.title,
                            "body": r.body.unwrap_or_default()
                        })
                    }
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&vals)?);
        }
        OutputFormat::Files => {
            for r in &results {
                if r.skipped {
                    println!("{} [SKIPPED]", r.file);
                } else {
                    println!("{}", r.file);
                }
            }
        }
        OutputFormat::Csv => {
            println!("file,title,skipped,body_or_reason");
            for r in &results {
                let body_or = if r.skipped {
                    r.skip_reason.clone().unwrap_or_default()
                } else {
                    r.body.clone().unwrap_or_default()
                };
                println!(
                    "{},{},{},{}",
                    escape_csv(&r.file),
                    escape_csv(&r.title),
                    if r.skipped { "true" } else { "false" },
                    escape_csv(&body_or)
                );
            }
        }
        OutputFormat::Md => {
            for r in &results {
                println!("## {}\n", r.file);
                if r.title != r.file {
                    println!("**Title:** {}\n", r.title);
                }
                if r.skipped {
                    println!("> {}\n", r.skip_reason.clone().unwrap_or_default());
                } else {
                    println!("```\n{}\n```\n", r.body.as_deref().unwrap_or(""));
                }
            }
        }
        OutputFormat::Xml => {
            println!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
            println!("<documents>");
            for r in &results {
                println!("  <document>");
                println!("    <file>{}</file>", escape_xml(&r.file));
                println!("    <title>{}</title>", escape_xml(&r.title));
                if r.skipped {
                    println!("    <skipped>true</skipped>");
                    println!(
                        "    <reason>{}</reason>",
                        escape_xml(&r.skip_reason.clone().unwrap_or_default())
                    );
                } else {
                    println!(
                        "    <body>{}</body>",
                        escape_xml(r.body.as_deref().unwrap_or(""))
                    );
                }
                println!("  </document>");
            }
            println!("</documents>");
        }
        _ => {
            // Text / default CLI format (matches reference separators + layout)
            for r in &results {
                println!("\n{}", "=".repeat(60));
                println!("File: {}", r.file);
                println!("{}\n", "=".repeat(60));

                if r.skipped {
                    println!("[SKIPPED: {}]", r.skip_reason.clone().unwrap_or_default());
                    continue;
                }

                println!("{}", r.body.as_deref().unwrap_or(""));
            }
            if !results.is_empty() {
                println!();
            }
        }
    }
    Ok(())
}

/// Entry point wired from main.rs.
pub fn cmd_multi_get(
    pattern: String,
    l: Option<usize>,
    max_bytes: Option<usize>,
    format: OutputFormat,
) -> Result<()> {
    let input = pattern.trim();
    if input.is_empty() {
        eprintln!("multi-get: empty pattern");
        return Ok(());
    }

    let conn = open_connection(true).context("failed to open index for multi-get")?;

    let cands = if is_comma_list(input) {
        resolve_comma_list(&conn, input)?
    } else {
        resolve_glob(&conn, input)?
    };

    let maxb = max_bytes.unwrap_or(10 * 1024);
    let mut out: Vec<MultiResult> = Vec::new();
    for c in cands {
        if let Some(r) = fetch_and_limit(&conn, c, maxb, l)? {
            out.push(r);
        }
    }

    output_results(out, format)?;
    Ok(())
}
