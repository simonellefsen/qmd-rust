//! `qmd embed` implementation — real embedding generation (Area 2 sub-slice #1-#4).
//!
//! Uses the pluggable `Embedder` trait. When the `llama-embed` feature is enabled,
//! `LlamaEmbedder` loads a real GGUF model (via QMD_EMBED_MODEL or config) and
//! produces meaningful vectors. Fingerprinting (`embedding_fingerprint`) + early
//! COUNT check enables skip-on-match for unchanged content (cheap repeated runs).
//! The discovery → chunk → (fp-gated) embed → store pipeline is fully exercised.
//! Called by `qmd embed` and by `qmd update --embed`.

use crate::db::load_config;
use crate::embed;
use crate::embed::Embedder;
use crate::index::{discover_files, simple_chunk, store_vectors};
use anyhow::Result;

pub fn cmd_embed(
    force: bool,
    collection: Option<String>,
    _chunk_strategy: crate::cli::args::ChunkStrategy,
) -> Result<()> {
    let embedder: Box<dyn Embedder> = embed::default_embedder();
    let model = embedder.model_id().to_string();
    let dim = embedder.dimension();

    println!("Embedding with model: {} (dim={})", model, dim);

    if dim == 0 {
        println!("No-op embedder — nothing to do (enable `llama-embed` feature for real vectors).");
        return Ok(());
    }

    if force {
        println!("(force flag noted)");
    }

    let cfg = load_config().unwrap_or_default();
    let collections = cfg.collections.unwrap_or_default();

    let targets: Vec<_> = if let Some(name) = &collection {
        collections.into_iter().filter(|(k, _)| k == name).collect()
    } else {
        collections.into_iter().collect()
    };

    let mut total_chunks = 0;

    for (name, coll_cfg) in targets {
        println!("Embedding collection '{}' ...", name);

        let ignores: Vec<String> = coll_cfg.ignore_patterns.clone().unwrap_or_default();
        let files = match discover_files(&coll_cfg.path, &coll_cfg.pattern, &ignores) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("  Failed to discover files: {}", e);
                continue;
            }
        };

        for path in files {
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let hash = crate::index::content_hash(&content);
            let fingerprint = crate::index::embedding_fingerprint(
                &model,
                crate::index::EMBED_CHUNKER_TOKEN,
                crate::index::EMBED_FMT_VER,
            );

            // Skip unchanged content when fingerprint matches (unless --force).
            // This is the core of better fingerprinting for Area 2 (#2).
            if !force {
                if let Ok(conn) = crate::db::open_connection(true) {
                    if let Ok(cnt) = conn.query_row(
                        "SELECT COUNT(*) FROM content_vectors WHERE hash = ?1 AND embed_fingerprint = ?2",
                        [&hash, &fingerprint],
                        |r| r.get::<_, i32>(0),
                    ) {
                        if cnt > 0 {
                            // already embedded with current model+chunker+fmt
                            continue;
                        }
                    }
                }
            }

            // Stale vector hygiene (#4): when we are about to re-embed this content hash
            // (fp changed or force), remove any prior vectors for it (old fp or extra seqs
            // from a previous larger chunk count). This keeps exactly the current seqs under
            // the current fp for the active document. store_vectors will then insert fresh.
            if let Ok(conn) = crate::db::open_connection(false) {
                let _ = conn.execute("DELETE FROM content_vectors WHERE hash = ?1", [&hash]);
            }

            let chunks = simple_chunk(&content, 800);
            if chunks.is_empty() {
                continue;
            }

            let vecs = match embedder
                .embed_batch(&chunks.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("  Embedding failed for {}: {}", path.display(), e);
                    continue;
                }
            };

            for (i, vec) in vecs.into_iter().enumerate() {
                if let Err(e) = store_vectors(&hash, i as i32, &model, &vec, &fingerprint) {
                    eprintln!("  Failed to store vector {}#{}: {}", path.display(), i, e);
                } else {
                    total_chunks += 1;
                }
            }
        }
    }

    println!("✓ Embedded {} chunks.", total_chunks);
    Ok(())
}
