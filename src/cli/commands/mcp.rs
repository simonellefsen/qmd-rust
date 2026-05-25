//! Minimal MCP stdio server implementation for `qmd mcp`.
//!
//! Provides a functional JSON-RPC loop for a few tools (status, get, query, multi_get (comma lists via reuse of get_body_from_db)).
//! HTTP/daemon modes stubbed. Duplicates some get logic for the MCP tool handler (as in original).

use crate::db::search as db_search;
use crate::db::{active_db_path, db_counts, load_config};
use anyhow::Result;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};

use super::{get_body_from_db, parse_structured_query, ClauseKind, ParsedQuery};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Handle `qmd mcp [--http] [--port] [--daemon]`
pub fn cmd_mcp(http: bool, _port: u16, daemon: bool) -> Result<()> {
    if http || daemon {
        eprintln!("MCP --http/--daemon not implemented in Rust port yet.");
        eprintln!("Use the reference binary (if available) or cargo run -- mcp (for stdio)");
        return Ok(());
    }
    eprintln!("[qmd-rust] MCP stdio server starting (tools: status, get, query (lex + structured), multi_get (comma); richer structuredContent for agents)");
    run_mcp_stdio_loop()
}

fn run_mcp_stdio_loop() -> Result<()> {
    // Acquire exclusive lock on stdin for the lifetime of the stdio MCP transport.
    // This is the standard pattern for line-delimited JSON-RPC servers (prevents interleaving with any other stdin use).
    let stdin = io::stdin();
    let lock = stdin.lock(); // explicit guard for lifetime clarity (no functional change)
    let reader = BufReader::new(lock);
    let mut lines = reader.lines();
    loop {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            Some(Err(_)) => break,
            None => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                // Emit proper JSON-RPC parse error (id null is acceptable per spec for parse failures)
                let err = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": "Parse error" }
                });
                println!("{}", serde_json::to_string(&err).unwrap_or_default());
                let _ = io::stdout().flush();
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let resp = match method {
            "initialize" => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": { "name": "qmd-rust", "version": VERSION }
                    }
                })
            }
            "tools/list" => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "status",
                                "description": "Return QMD index status (documents, vectors, collections, version, rust). Returns structuredContent (minimal payload for agent consumption).",
                                "inputSchema": { "type": "object", "properties": {} }
                            },
                            {
                                "name": "get",
                                "description": "Retrieve document content by qmd:// path, collection/path, or #docid. Supports :line suffix or l/fromLine. Returns text (or resource for MCP clients).",
                                "inputSchema": { "type": "object", "properties": { "file": { "type": "string" }, "l": {"type":"number"}, "fromLine": {"type":"number"}, "maxLines": {"type":"number"}, "line_numbers": {"type":"boolean"} } }
                            },
                            {
                                "name": "multi_get",
                                "description": "Batch retrieve by comma-separated list of paths/docids (glob patterns supported via CLI multi-get). Returns structured list + text summary. Skips oversized if applicable.",
                                "inputSchema": { "type": "object", "properties": { "pattern": { "type": "string" }, "maxLines": {"type":"number"}, "line_numbers": {"type":"boolean"} } }
                            },
                            {
                                "name": "query",
                                "description": "Lexical (BM25/FTS5) search with structured output. 'query' supports plain text or grammar from docs/SYNTAX.md: lex:, intent:, vec:/hyde: (graceful note if no embeddings). Returns hits as structuredContent array (docid, file, title, score, snippet) + text summary. Params: n, collection (or collections), min_score, intent (for future disambig). Recommended for agents: use intent + lex first.",
                                "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "n": {"type":"number"}, "collection": {"type":"string"}, "min_score": {"type":"number"}, "intent": {"type":"string"} } }
                            }
                        ]
                    }
                })
            }
            "tools/call" => {
                let tname = req
                    .get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args = req
                    .get("params")
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                // Per-tool: text for human/agent summary + optional structuredContent (richer output for LLM agents per deeper MCP goal; reuses FtsHit + helpers)
                let (text, structured) = match tname {
                    "status" => {
                        // best effort + richer structured (docs/vecs/colls + version); matches extended status --json spirit without new imports
                        let cfg = load_config().unwrap_or_default();
                        let (docs, vecs) = db_counts(&active_db_path()).unwrap_or((0, 0));
                        let colls: Vec<_> = cfg
                            .collections
                            .as_ref()
                            .map(|m| m.keys().cloned().collect())
                            .unwrap_or_default();
                        let status_obj = serde_json::json!({
                            "documents": docs,
                            "vectors": vecs,
                            "collections": colls,
                            "version": VERSION,
                            "rust": true
                        });
                        let txt = format!(
                            "QMD Rust status: {} docs, {} vectors, collections: {:?}",
                            docs, vecs, colls
                        );
                        (txt, Some(status_obj))
                    }
                    "get" => {
                        let mut f = args
                            .get("file")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let mut start_line: usize = 1;
                        if let Some(pos) = f.rfind(':') {
                            if let Ok(n) = f[pos + 1..].parse::<usize>() {
                                if n > 0 {
                                    start_line = n;
                                    f = f[..pos].to_string();
                                }
                            }
                        }
                        // support fromLine/maxLines aliases (TS parity) + existing l
                        if start_line == 1 {
                            if let Some(fl) = args
                                .get("fromLine")
                                .and_then(|x| x.as_u64())
                                .map(|v| v as usize)
                            {
                                if fl > 0 {
                                    start_line = fl;
                                }
                            }
                        }
                        let max_l = args
                            .get("l")
                            .and_then(|x| x.as_u64())
                            .map(|v| v as usize)
                            .or_else(|| {
                                args.get("maxLines")
                                    .and_then(|x| x.as_u64())
                                    .map(|v| v as usize)
                            });
                        let line_numbers = args
                            .get("line_numbers")
                            .and_then(|x| x.as_bool())
                            .unwrap_or(false);
                        // DB first, then disk (parity with CLI get)
                        let body = if let Some(b) = get_body_from_db(&f) {
                            b
                        } else {
                            // minimal disk resolve for MCP
                            let fs_path = if let Some(stripped) = f.strip_prefix("~/") {
                                if let Some(home) = env::var_os("HOME") {
                                    format!("{}/{}", home.to_string_lossy(), stripped)
                                } else {
                                    f.clone()
                                }
                            } else if !f.starts_with('/') && !f.starts_with('~') {
                                let cwd = env::current_dir()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                if cwd.is_empty() {
                                    f.clone()
                                } else {
                                    format!("{}/{}", cwd, f)
                                }
                            } else {
                                f.clone()
                            };
                            fs::read_to_string(&fs_path)
                                .unwrap_or_else(|_| format!("(not found on disk: {})", fs_path))
                        };
                        // slice + numbers (reuse safe logic pattern)
                        let all_lines: Vec<&str> = body.lines().collect();
                        let start_idx = start_line.saturating_sub(1).min(all_lines.len());
                        let end_idx = if let Some(ml) = max_l {
                            (start_idx + ml).min(all_lines.len())
                        } else {
                            all_lines.len()
                        };
                        let selected = &all_lines[start_idx..end_idx.min(all_lines.len())];
                        let txt = if line_numbers {
                            selected
                                .iter()
                                .enumerate()
                                .map(|(i, ln)| format!("{}: {}", start_line + i, ln))
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            selected.join("\n")
                        };
                        // For agents: could return resource type, but keep text + structured metadata for smallest
                        let get_struct = serde_json::json!({ "file": f, "start_line": start_line, "lines_returned": selected.len() });
                        (txt, Some(get_struct))
                    }
                    "query" => {
                        let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                        let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                        let coll = args.get("collection").and_then(|v| v.as_str());
                        // min_score and intent accepted for schema parity / future (min_score filter applied post-fts here for MCP slice)
                        let min_score = args
                            .get("min_score")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0) as f32;
                        // intent parsed but not yet used in fts path (see wontfix in wiki log)

                        // Compute effective search text (lex only for this MCP slice) and handle vec-only graceful note
                        let (search_text, note) = match parse_structured_query(q) {
                            Ok(ParsedQuery::Simple(text)) => (text, None),
                            Ok(ParsedQuery::Structured { clauses, .. }) => {
                                let lex_parts: Vec<&str> = clauses
                                    .iter()
                                    .filter(|c| c.kind == ClauseKind::Lex)
                                    .map(|c| c.text.as_str())
                                    .collect();
                                if lex_parts.is_empty() {
                                    (String::new(), Some("Vector/HyDE search requires embeddings (run qmd embed or use CLI query for full hybrid).".to_string()))
                                } else {
                                    (lex_parts.join(" "), None)
                                }
                            }
                            Err(_) => (q.to_string(), None),
                        };
                        let (txt, structured) = if let Some(nmsg) = note {
                            (
                                nmsg,
                                Some(
                                    serde_json::json!({"note": "vec/hyde require embeddings; MCP query is lex-focused in this slice; use CLI for rerank/expansion"}),
                                ),
                            )
                        } else if search_text.is_empty() {
                            ("empty query".to_string(), None)
                        } else if let Ok(hits) = db_search::fts_search(&search_text, n, coll) {
                            let filtered: Vec<_> =
                                hits.into_iter().filter(|h| h.score >= min_score).collect();
                            // Build human text summary directly from FtsHit (avoids post-map JSON roundtrips + unwraps)
                            let summary_lines: Vec<String> = filtered
                                .iter()
                                .map(|h| {
                                    format!(
                                        "{} {} (score {:.0}%)",
                                        h.file,
                                        h.docid,
                                        h.score * 100.0
                                    )
                                })
                                .collect();
                            let t = if summary_lines.is_empty() {
                                format!("No results (min_score={})", min_score)
                            } else {
                                summary_lines.join("\n")
                            };
                            let results: Vec<serde_json::Value> = filtered
                                .iter()
                                .map(|h| {
                                    serde_json::json!({
                                        "docid": h.docid,
                                        "file": h.file,
                                        "title": h.title,
                                        "score": h.score,
                                        "snippet": h.snippet
                                    })
                                })
                                .collect();
                            (
                                t,
                                Some(
                                    serde_json::json!({ "results": results, "count": results.len(), "query": q }),
                                ),
                            )
                        } else {
                            ("search error".to_string(), None)
                        };
                        (txt, structured)
                    }
                    "multi_get" => {
                        // Functional impl for comma lists (smallest viable; reuses get_body_from_db exactly; glob + advanced limits deferred to avoid editing multi_get.rs or adding deps here)
                        let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                        let line_numbers = args
                            .get("line_numbers")
                            .and_then(|x| x.as_bool())
                            .unwrap_or(false);
                        if pat.is_empty() {
                            ("no pattern provided".to_string(), None)
                        } else {
                            let items: Vec<&str> = pat
                                .split(',')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .collect();
                            let mut blocks: Vec<String> = vec![];
                            let mut docs: Vec<serde_json::Value> = vec![];
                            for item in items {
                                // Single call to get_body_from_db (fixes double-lookup nit); compute found + body_str once
                                let body_opt = get_body_from_db(item);
                                let found = body_opt.is_some();
                                let body_str = body_opt
                                    .unwrap_or_else(|| format!("(not found in index: {})", item));
                                // basic line_numbers support for parity (applies to whole body here; full per-doc slicing in CLI)
                                let out_body = if line_numbers {
                                    body_str
                                        .lines()
                                        .enumerate()
                                        .map(|(i, ln)| format!("{}: {}", i + 1, ln))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                } else {
                                    body_str.clone()
                                };
                                blocks.push(format!("=== {} ===\n{}", item, out_body));
                                docs.push(serde_json::json!({ "file": item, "found": found, "length": body_str.len() }));
                            }
                            let txt = blocks.join("\n\n");
                            (
                                txt,
                                Some(serde_json::json!({ "docs": docs, "count": docs.len() })),
                            )
                        }
                    }
                    _ => (format!("unknown tool: {}", tname), None),
                };
                let mut result = serde_json::json!({
                    "content": [ { "type": "text", "text": text } ]
                });
                if let Some(s) = structured {
                    result["structuredContent"] = s;
                }
                if text.starts_with("unknown tool: ") {
                    result["isError"] = serde_json::json!(true);
                }
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                })
            }
            "notifications/initialized" | "exit" => {
                // no response for notifications
                continue;
            }
            _ => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": "Method not found" }
                })
            }
        };
        let out = serde_json::to_string(&resp)?;
        println!("{}", out);
        let _ = io::stdout().flush(); // best-effort; errors are non-fatal for stdio transport (MCP clients tolerant)
    }
    Ok(())
}
