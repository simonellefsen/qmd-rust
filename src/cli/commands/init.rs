//! Implementation of `qmd init` (project-local .qmd/ index).
//!
//! Creates .qmd/index.yml + .qmd/index.sqlite in the current directory
//! (when --force or the dir does not yet exist).
//! Subsequent commands (status, collection, ls, etc.) automatically prefer
//! the local index when CWD is inside a tree containing .qmd/.
//!
//! Follows the exact per-command module pattern from context.rs / multi_get.rs.
//! Schema bootstrap reuses the minimal CREATEs established for hermetic tests.

use anyhow::Result;
use std::fs;
use std::path::Path;

/// Bootstrap the minimal tables required for a fresh local index.
/// Mirrors the schema in index::tests (kept in sync for CI hermeticity).
fn ensure_schema(db_path: &Path) -> Result<()> {
    let conn = rusqlite::Connection::open(db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS content (
            hash TEXT PRIMARY KEY,
            doc TEXT NOT NULL,
            created_at TEXT
        );
        CREATE TABLE IF NOT EXISTS documents (
            id INTEGER PRIMARY KEY,
            collection TEXT NOT NULL,
            path TEXT NOT NULL,
            title TEXT,
            hash TEXT,
            created_at TEXT,
            modified_at TEXT,
            active INTEGER DEFAULT 1,
            UNIQUE(collection, path)
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
            filepath, title, body, tokenize='unicode61'
        );
        CREATE TABLE IF NOT EXISTS store_collections (
            name TEXT PRIMARY KEY,
            path TEXT,
            mask TEXT,
            context TEXT,
            last_indexed TEXT
        );
        CREATE TABLE IF NOT EXISTS content_vectors (
            hash TEXT,
            embedding BLOB
        );
        "#,
    )?;
    Ok(())
}

/// Entry point for `qmd init [--force]`.
pub fn cmd_init(force: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let qmd_dir = cwd.join(".qmd");
    let yml_path = qmd_dir.join("index.yml");
    let db_path = qmd_dir.join("index.sqlite");

    let dir_exists = qmd_dir.is_dir();

    if dir_exists && !force {
        println!("Local .qmd/ index already present at {}", qmd_dir.display());
        println!("(Use --force to reinitialize.)");
        return Ok(());
    }

    fs::create_dir_all(&qmd_dir)?;

    if !yml_path.exists() || force {
        // Minimal viable config (collections empty; models can be added later or via env).
        fs::write(&yml_path, "collections: {}\n")?;
    }

    // Ensure DB file + schema so the local index is immediately usable.
    ensure_schema(&db_path)?;

    println!("Initialized local QMD index at {}", qmd_dir.display());
    println!("  - {}", yml_path.display());
    println!("  - {}", db_path.display());
    println!();
    println!("Next steps (examples only; do not auto-execute):");
    println!("  qmd collection add . --name notes");
    println!("  qmd status");
    println!("  qmd update");
    Ok(())
}
