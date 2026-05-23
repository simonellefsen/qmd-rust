//! Minimal MCP stdio server implementation for `qmd mcp`.
//!
//! Provides a functional JSON-RPC loop for a few tools (status, get, query, multi_get stub).
//! HTTP/daemon modes stubbed. Duplicates some get logic for the MCP tool handler (as in original).

use crate::db::search as db_search;
use crate::db::{db_counts, load_config};
use anyhow::Result;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};

use super::{get_body_from_db, parse_structured_query, ClauseKind, ParsedQuery};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const INDEX_PATH: &str = "~/.cache/qmd/index.sqlite";

/// Handle `qmd mcp [--http] [--port] [--daemon]`
pub fn cmd_mcp(http: bool, _port: u16, daemon: bool) -> Result<()> {
    if http || daemon {
        eprintln!("MCP --http/--daemon not implemented in Rust port yet.");
        eprintln!("Use: /opt/homebrew/bin/qmd mcp   or cargo run -- mcp (for stdio)");
        return Ok(());
    }
    eprintln!("[qmd-rust] minimal MCP stdio server starting (tools: status, get, query[lex], multi_get stub)");
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
                                "description": "Return QMD index status (docs, collections, models)",
                                "inputSchema": { "type": "object", "properties": {} }
                            },
                            {
                                "name": "get",
                                "description": "Retrieve document content by qmd:// path or #docid",
                                "inputSchema": { "type": "object", "properties": { "file": { "type": "string" }, "l": {"type":"number"}, "line_numbers": {"type":"boolean"} } }
                            },
                            {
                                "name": "multi_get",
                                "description": "Not fully implemented in this minimal MCP; use Node reference for now",
                                "inputSchema": { "type": "object", "properties": { "pattern": { "type": "string" } } }
                            },
                            {
                                "name": "query",
                                "description": "Lexical (BM25/FTS5) search. 'query' can be plain text or structured (lex: ..., intent: ..., vec:/hyde: for future). vec/hyde currently return a note.",
                                "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "n": {"type":"number"}, "collection": {"type":"string"} } }
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
                let text = match tname {
                    "status" => {
                        // best effort text status
                        let cfg = load_config().unwrap_or_default();
                        let (docs, vecs) = db_counts(INDEX_PATH).unwrap_or((0, 0));
                        let colls: Vec<_> = cfg
                            .collections
                            .as_ref()
                            .map(|m| m.keys().cloned().collect())
                            .unwrap_or_default();
                        format!(
                            "QMD Rust status: {} docs, {} vectors, collections: {:?}",
                            docs, vecs, colls
                        )
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
                        let max_l = args.get("l").and_then(|x| x.as_u64()).map(|v| v as usize);
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
                        if line_numbers {
                            selected
                                .iter()
                                .enumerate()
                                .map(|(i, ln)| format!("{}: {}", start_line + i, ln))
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            selected.join("\n")
                        }
                    }
                    "query" => {
                        let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                        let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                        let coll = args.get("collection").and_then(|v| v.as_str());

                        match parse_structured_query(q) {
                            Ok(ParsedQuery::Simple(text)) => {
                                if let Ok(hits) = db_search::fts_search(&text, n, coll) {
                                    hits.iter()
                                        .map(|h| {
                                            format!(
                                                "{} {} (score {:.0}%)",
                                                h.file,
                                                h.docid,
                                                h.score * 100.0
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                } else {
                                    "search error".to_string()
                                }
                            }
                            Ok(ParsedQuery::Structured { clauses, .. }) => {
                                let lex_parts: Vec<&str> = clauses
                                    .iter()
                                    .filter(|c| c.kind == ClauseKind::Lex)
                                    .map(|c| c.text.as_str())
                                    .collect();

                                if lex_parts.is_empty() {
                                    "Vector/HyDE search requires embeddings (Area 2 / v0.4.0)."
                                        .to_string()
                                } else {
                                    let joined = lex_parts.join(" ");
                                    if let Ok(hits) = db_search::fts_search(&joined, n, coll) {
                                        hits.iter()
                                            .map(|h| {
                                                format!(
                                                    "{} {} (score {:.0}%)",
                                                    h.file,
                                                    h.docid,
                                                    h.score * 100.0
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                    } else {
                                        "search error".to_string()
                                    }
                                }
                            }
                            Err(_) => {
                                // backward compat for plain queries
                                if let Ok(hits) = db_search::fts_search(q, n, coll) {
                                    hits.iter()
                                        .map(|h| {
                                            format!(
                                                "{} {} (score {:.0}%)",
                                                h.file,
                                                h.docid,
                                                h.score * 100.0
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                } else {
                                    "search error".to_string()
                                }
                            }
                        }
                    }
                    "multi_get" => {
                        "multi_get not implemented in minimal Rust MCP (use Node)".to_string()
                    }
                    _ => format!("unknown tool: {}", tname),
                };
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [ { "type": "text", "text": text } ]
                    }
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
