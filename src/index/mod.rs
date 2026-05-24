//! Basic file discovery and content indexing logic (Area 2 foundation).
//!
//! Goal for first slice (0.4.0):
//! - Walk collections using their path + glob pattern.
//! - Insert discovered markdown files into `documents` + `content` tables.
//! - Support `qmd update` (with optional --pull).
//!
//! Vector/embedding generation will be layered on top in later slices of this area.

use crate::db::open_connection;
use anyhow::Result;
use globset::{Glob, GlobSetBuilder};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Discover files for a single collection according to its pattern.
/// Respects built-in ignored dirs plus any per-collection `ignore_patterns` (from YAML).
pub fn discover_files(
    collection_path: &str,
    pattern: &str,
    ignore_patterns: &[String],
) -> Result<Vec<PathBuf>> {
    let root = Path::new(collection_path);

    if !root.exists() {
        return Ok(vec![]);
    }

    // Build glob matcher from the collection pattern (e.g. "**/*.md")
    let mut builder = GlobSetBuilder::new();
    builder.add(Glob::new(pattern)?);
    let matcher = builder.build()?;

    // Per-collection ignores (in addition to built-in).
    // Invalid globs are skipped gracefully (best-effort) so a single typo in ignore_patterns
    // does not abort indexing of the whole collection.
    let mut ignore_builder = GlobSetBuilder::new();
    for pat in ignore_patterns {
        if let Ok(g) = Glob::new(pat) {
            ignore_builder.add(g);
        }
    }
    let ignore_matcher = ignore_builder.build().ok(); // if empty or bad, ignore errors gracefully

    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e.path()))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_file() {
            let relative = match entry.path().strip_prefix(root) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let rel_str = relative.to_string_lossy().replace('\\', "/");

            if matcher.is_match(&rel_str) {
                // Also skip obvious hidden / build dirs at file level
                if !rel_str.split('/').any(|part| part.starts_with('.')) {
                    let ignored = ignore_matcher
                        .as_ref()
                        .is_some_and(|m| m.is_match(&rel_str));
                    if !ignored {
                        files.push(entry.path().to_path_buf());
                    }
                }
            }
        }
    }

    Ok(files)
}

fn is_ignored_dir(path: &Path) -> bool {
    const IGNORED: &[&str] = &[
        "node_modules",
        ".git",
        ".cache",
        "target",
        "dist",
        "build",
        ".next",
        ".svelte-kit",
    ];

    path.components().any(|c| {
        if let std::path::Component::Normal(os_str) = c {
            if let Some(s) = os_str.to_str() {
                return IGNORED.contains(&s);
            }
        }
        false
    })
}

/// Very basic title extraction (first # heading or filename).
pub fn extract_title(path: &Path, content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            return trimmed.trim_start_matches("# ").trim().to_string();
        }
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

/// Compute a stable content hash (blake3 is fast and good enough).
pub fn content_hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

/// Insert or update a single file in the index (raw content, no vectors yet).
pub fn upsert_document(
    collection: &str,
    relative_path: &str,
    absolute_path: &Path,
    content: &str,
) -> Result<()> {
    let mut conn = open_connection(false)?;

    let title = extract_title(absolute_path, content);
    let hash = content_hash(content);
    let now = chrono::Utc::now().to_rfc3339();

    let tx = conn.transaction()?;

    // Content first (FK from documents -> content.hash); OR IGNORE preserves original created_at
    tx.execute(
        r#"
        INSERT OR IGNORE INTO content (hash, doc, created_at)
        VALUES (?1, ?2, ?3)
        "#,
        rusqlite::params![&hash, content, &now],
    )?;

    // Robust upsert: ON CONFLICT keeps stable row id (per UNIQUE(collection, path))
    // and sets active=1. This properly handles "replacing" prior version of the path.
    // (Deactivate logic is used elsewhere for files removed from FS.)
    tx.execute(
        r#"
        INSERT INTO documents (collection, path, title, hash, created_at, modified_at, active)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
        ON CONFLICT(collection, path) DO UPDATE SET
            title = excluded.title,
            hash = excluded.hash,
            modified_at = excluded.modified_at,
            active = 1
        "#,
        rusqlite::params![collection, relative_path, &title, &hash, &now, &now],
    )?;

    // Sync FTS (trigger only fires on INSERT; updates to existing row require manual rebuild
    // so that search continues to see the new title/body for this doc id)
    let doc_id: i64 = tx.query_row(
        "SELECT id FROM documents WHERE collection = ?1 AND path = ?2",
        rusqlite::params![collection, relative_path],
        |r| r.get(0),
    )?;
    tx.execute("DELETE FROM documents_fts WHERE rowid = ?", [doc_id])?;
    let body: String = tx.query_row("SELECT doc FROM content WHERE hash = ?", [&hash], |r| {
        r.get(0)
    })?;
    let fts_path = format!("{}/{}", collection, relative_path);
    tx.execute(
        "INSERT INTO documents_fts (rowid, filepath, title, body) VALUES (?, ?, ?, ?)",
        rusqlite::params![doc_id, &fts_path, &title, &body],
    )?;

    // Best-effort placeholder touch on store_collections (no-op today; safe even if row/column absent in current schema).
    // Future Area 2 slice will add a real last_indexed column + migration when the full pipeline needs it.
    let _ = tx.execute(
        "UPDATE store_collections SET name = name WHERE name = ?",
        [collection],
    );

    tx.commit()?;
    Ok(())
}

/// Store a vector for a specific chunk of a document.
/// For this slice we store the vector as a BLOB of f32s (little-endian) in content_vectors.
/// This keeps us compatible with the existing schema while avoiding the complexity of
/// loading the sqlite-vec extension for vec0 virtual tables in static builds.
pub fn store_vectors(
    hash: &str,
    seq: i32,
    model: &str,
    vector: &[f32],
    fingerprint: &str,
) -> Result<()> {
    let conn = open_connection(false)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Convert f32 slice to bytes (little endian)
    let bytes: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();

    conn.execute(
        r#"
        INSERT INTO content_vectors (hash, seq, model, vector, embed_fingerprint, embedded_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(hash, seq) DO UPDATE SET
            model = excluded.model,
            vector = excluded.vector,
            embed_fingerprint = excluded.embed_fingerprint,
            embedded_at = excluded.embedded_at
        "#,
        rusqlite::params![hash, seq, model, &bytes, fingerprint, &now],
    )?;

    Ok(())
}

/// Very simple chunker for the first embedding slice.
/// Splits on double newlines (paragraphs) and falls back to fixed-size chunks.
pub fn simple_chunk(content: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    for para in content.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if para.len() <= max_chars {
            chunks.push(para.to_string());
        } else {
            // crude fixed window chunking
            let mut start = 0;
            while start < para.len() {
                let end = (start + max_chars).min(para.len());
                chunks.push(para[start..end].to_string());
                start = end;
            }
        }
    }
    if chunks.is_empty() {
        chunks.push(content.to_string());
    }
    chunks
}

/// Compute a compact embedding fingerprint from model id + chunking parameters + format version.
/// Used to detect when re-embedding is required (changed model, chunker, or embedding logic).
/// Short 8-char blake3 prefix for storage in content_vectors.embed_fingerprint.
/// This is the Rust equivalent of the significant fields hashed in the original
/// getEmbeddingFingerprint (model + formatting probes + chunk_*_tokens).
pub fn embedding_fingerprint(model: &str, chunker: &str, fmt_ver: &str) -> String {
    let sig = format!(
        "model:{}\nchunker:{}\nfmt:{}\nslice:area2-real-embed-1",
        model, chunker, fmt_ver
    );
    let h = blake3::hash(sig.as_bytes()).to_hex().to_string();
    h[..8].to_string()
}

/// Current chunker token used by the basic embed path (matches `simple_chunk(800)`).
/// Centralized here per review observation so the smart-chunker slice has one place to update.
pub const EMBED_CHUNKER_TOKEN: &str = "simple-800";
/// Current format version token for embedding fingerprints in this sub-slice.
pub const EMBED_FMT_VER: &str = "1";

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_update_path_end_to_end_with_ignore_patterns() {
        // Exercises A-E: discover + upsert (with ignores from C), improved upsert (B),
        // via temp dir as collection root. Uses unique coll name to avoid clobber.
        //
        // Self-contained schema bootstrap so the test works even on a completely fresh
        // SQLite file (important for CI where no prior qmd run has created tables).
        let pid = std::process::id();
        let tmp = std::env::temp_dir().join(format!("qmd-rust-test-idx-{}", pid));
        // fresh dir
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Ensure minimal schema exists (tables touched by upsert_document + FTS).
        // This makes the test hermetic and removes the hidden dependency on the TS
        // version having run previously.
        {
            let conn = crate::db::open_connection(false).unwrap();
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
                "#,
            ).unwrap();
        }

        // good files
        fs::write(
            tmp.join("note1.md"),
            "# Test Note 1\nHello from Rust update test.",
        )
        .unwrap();
        fs::write(
            tmp.join("note2.md"),
            "# Test Note 2\nSecond doc for end-to-end.",
        )
        .unwrap();

        // ignored subdir + file (exercises C)
        fs::create_dir_all(tmp.join("drafts")).unwrap();
        fs::write(
            tmp.join("drafts/secret.md"),
            "# Secret draft\nShould be ignored.",
        )
        .unwrap();

        let coll = format!("test_update_{}", pid);
        let ignores: Vec<String> = vec!["drafts/**".to_string()];

        // discover with ignores
        let files = discover_files(tmp.to_str().unwrap(), "**/*.md", &ignores).unwrap();
        // should find exactly the 2 good files
        assert_eq!(
            files.len(),
            2,
            "ignore_patterns should have filtered the draft"
        );

        // run the upsert path for each (exercises A/B core)
        for abs in &files {
            let rel = abs
                .strip_prefix(&tmp)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(abs).unwrap();
            upsert_document(&coll, &rel, abs, &content).unwrap();
        }

        // assert documents appear in DB (active) — use writable conn so cleanup can actually run
        let conn = crate::db::open_connection(false).unwrap();
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM documents WHERE collection = ? AND active = 1",
                [&coll],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 2, "expected >=2 indexed docs, got {}", count);

        // cleanup fs temp
        let _ = fs::remove_dir_all(&tmp);
        // Thorough cleanup for true isolation on the shared index used by CI and local dev.
        let _ = conn.execute("DELETE FROM documents WHERE collection = ?", [&coll]);
        let _ = conn.execute("DELETE FROM store_collections WHERE name = ?", [&coll]);
        // Also clean content and FTS entries that might be referenced by the test docs.
        let _ = conn.execute(
            "DELETE FROM content WHERE hash IN (SELECT hash FROM documents WHERE collection = ?)",
            [&coll],
        );
        // Note: documents_fts is a virtual table; rowids are tied to documents.id, so deleting
        // the documents rows above is usually sufficient. Explicit cleanup kept minimal.
    }
}
