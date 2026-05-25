//! Implementation of `qmd update` (first slice of Area 2).
//!
//! For the initial 0.4.0 deliverable we focus on:
//! - Walking collections
//! - Inserting raw content + metadata
//! - (Vectors will come in a later slice of this area)

use crate::cli::args::ChunkStrategy;
use crate::db::{expand_tilde, load_config};
use crate::index::{discover_files, upsert_document};
use anyhow::Result;

/// Handle `qmd update [--pull] [--embed] [--chunk-strategy]`
pub fn cmd_update(pull: bool, embed: bool, chunk_strategy: ChunkStrategy) -> Result<()> {
    if pull {
        eprintln!("Note: --pull is not yet implemented in Rust (use the reference binary or run git pull manually).");
    }

    let cfg = load_config().unwrap_or_default();
    let collections = cfg.collections.unwrap_or_default();

    if collections.is_empty() {
        println!("No collections configured. Use `qmd collection add <path>` first.");
        return Ok(());
    }

    let mut total_indexed = 0;

    for (name, coll) in &collections {
        println!("Updating collection '{}' (qmd://{}/) ...", name, name);

        let coll_path = expand_tilde(&coll.path);
        let ignores = coll.ignore_patterns.as_deref().unwrap_or(&[]);
        let files = discover_files(&coll_path, &coll.pattern, ignores)?;

        let mut indexed_in_coll = 0;

        for abs_path in files {
            let relative = match abs_path.strip_prefix(&coll_path) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };

            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("  Skipped unreadable {}: {}", relative, e);
                    continue;
                }
            };

            if let Err(e) = upsert_document(name, &relative, &abs_path, &content) {
                eprintln!("  Failed to index {}: {}", relative, e);
                continue;
            }

            indexed_in_coll += 1;
        }

        println!("  Indexed/updated {} files", indexed_in_coll);
        total_indexed += indexed_in_coll;
    }

    println!(
        "\n✓ Update complete. Total files processed: {}",
        total_indexed
    );

    if embed {
        println!("--embed: delegating to embedding pipeline (fingerprints will skip unchanged chunks)...");
        // Reuse the existing embed command (it now does fingerprint-based skipping for #2).
        // This gives us automatic post-update embedding for new/changed content (#4)
        // with the smallest possible diff. Double discovery is acceptable for this slice.
        // cmd_embed returns Ok(()) today (all per-chunk errors are logged + continued inside);
        // future slices may return real errors for the caller.
        let _ = crate::cli::commands::embed::cmd_embed(false, None, chunk_strategy);
    } else {
        println!("  (vectors not generated; re-run with --embed or use `qmd embed` when `llama-embed` feature is enabled)");
    }

    println!("  Run `qmd status` or `qmd search ...` to verify.");

    Ok(())
}
