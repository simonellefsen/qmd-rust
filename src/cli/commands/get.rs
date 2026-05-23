//! Implementation of the `qmd get` command.
//!
//! Retrieves document by #docid, qmd:// path, collection/path, or falls back to filesystem.
//! Supports -l N, --full, --line-numbers.

use crate::db::open_connection;
use anyhow::Result;
use std::env;
use std::fs;

use super::get_body_from_db;

/// Handle `qmd get <file> [-l N] [--full] [--line-numbers]`
pub fn cmd_get(file: String, l: Option<usize>, full: bool, line_numbers: bool) -> Result<()> {
    let mut input = file.clone();
    let mut start_line: usize = 1;
    if let Some(pos) = input.rfind(':') {
        if let Ok(n) = input[pos + 1..].parse::<usize>() {
            if n > 0 {
                start_line = n;
                input = input[..pos].to_string();
            }
        }
    }

    let max_lines = if full { None } else { l };

    // docid fast path (# or short hex 3-8 chars): docid lookup has precedence over FS path of same name (rare hex collision; documented per review).
    // Matches TS isDocid + findDocumentByDocid priority.
    let body = if input.starts_with('#')
        || (input.len() <= 8 && input.chars().all(|c| c.is_ascii_hexdigit()))
    {
        if let Ok(conn) = open_connection(true) {
            let short = input.trim_start_matches('#');
            conn.query_row(
                "SELECT (SELECT doc FROM content WHERE hash = d.hash) FROM documents d WHERE d.hash LIKE ? AND d.active=1 LIMIT 1",
                [format!("{}%", short)],
                |r| r.get::<_, String>(0),
            ).ok()
        } else {
            None
        }
    } else {
        None
    };

    let body = if let Some(b) = body {
        b
    } else if let Some(b) = get_body_from_db(&input) {
        b
    } else {
        // disk fallback for any path (makes get universally useful)
        let fs_path = if let Some(stripped) = input.strip_prefix("~/") {
            if let Some(home) = env::var_os("HOME") {
                format!("{}/{}", home.to_string_lossy(), stripped)
            } else {
                input.clone()
            }
        } else if !input.starts_with('/') && !input.starts_with('~') {
            let cwd = env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            if cwd.is_empty() {
                input.clone()
            } else {
                format!("{}/{}", cwd, input)
            }
        } else {
            input.clone()
        };
        match fs::read_to_string(&fs_path) {
            Ok(s) => {
                println!("(read from disk: {})", fs_path);
                s
            }
            Err(_) => {
                eprintln!("Document not found: {}", file);
                std::process::exit(1);
            }
        }
    };

    let all_lines: Vec<&str> = body.lines().collect();
    let mut start_idx = start_line.saturating_sub(1);
    let mut end_idx = if let Some(ml) = max_lines {
        (start_idx + ml).min(all_lines.len())
    } else {
        all_lines.len()
    };
    if start_idx > all_lines.len() {
        start_idx = all_lines.len();
    }
    if end_idx < start_idx {
        end_idx = start_idx;
    }
    end_idx = end_idx.min(all_lines.len());
    let selected = &all_lines[start_idx..end_idx];

    let output = if line_numbers {
        selected
            .iter()
            .enumerate()
            .map(|(i, ln)| format!("{}: {}", start_line + i, ln))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        selected.join("\n")
    };

    println!("{}", output);
    Ok(())
}
