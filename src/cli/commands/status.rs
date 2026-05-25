//! Implementation of the `qmd status` command.
//!
//! Significantly improved for 0.5.0: now surfaces embed model (via the Area 2
//! embed infrastructure), config models, basic vector health, and warnings.
//! JSON output extended for machine consumers. Follows exact prior patterns
//! and reuses load_config + db helpers. I3 polish: editor_uri surfaced (effective
//! value from env/yaml) in text + JSON. No new path literals in new code.

use crate::db::{
    active_db_path, collection_vector_count, db_counts, editor_uri, get_collection_stats,
    last_updated_hint, load_config,
};
use crate::embed::default_embedder;
use anyhow::Result;

#[allow(dead_code)] // kept for future parity / extension (see review nit #10)
const CONFIG_DIR: &str = "~/.config/qmd";

/// Handle the `qmd status` (and `status --json`) command.
pub fn cmd_status(json: bool) -> Result<()> {
    let index = active_db_path();
    // CONFIG_DIR constant kept for future parity / extension (no new path literals rule).
    // active_* helpers (from db) provide local .qmd/ preference when present.

    let cfg = load_config().unwrap_or_default();
    let (doc_count, vec_count) = db_counts(&index).unwrap_or((0, 0));
    let updated = last_updated_hint(&index).unwrap_or_else(|| "unknown".to_string());

    let collection_count = cfg.collections.as_ref().map(|c| c.len()).unwrap_or(0);

    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Real embed info from the llama (or noop) embedder — this is the "new embed infrastructure"
    let embedder = default_embedder();
    let embed_model = embedder.model_id().to_string();
    let embed_dim = embedder.dimension();
    let real_embed = embed_dim > 0;

    // Models from config (embed/generate/rerank) for visibility (no Clone on ModelsCfg in this slice)
    let embed_m = cfg.models.as_ref().and_then(|m| m.embed.clone());
    let gen_m = cfg.models.as_ref().and_then(|m| m.generate.clone());
    let rer_m = cfg.models.as_ref().and_then(|m| m.rerank.clone());

    // Effective editor_uri (env QMD_EDITOR_URI wins over yaml editor_uri) for agent TTY polish (I3).
    // Empty means no special clickable support in search/query/get output.
    let editor = editor_uri().unwrap_or_default();

    // Basic vector health (presence + rough diversity)
    let vec_health = if vec_count == 0 && doc_count > 0 {
        "missing (consider update --embed)".to_string()
    } else if vec_count > 0 {
        "present".to_string()
    } else {
        "n/a".to_string()
    };

    // #4 richer observability (smallest viable, reuse only): per-collection embedding health
    // (docs + vectors via new helper following get_collection_stats exactly) + global
    // fingerprint status + model diagnostics (light queries, open_connection pattern).
    // Per-coll health gives embedding coverage signal per user collection.
    let mut per_coll: Vec<(String, u32, u32)> = Vec::new();
    if let Some(cols) = &cfg.collections {
        for name in cols.keys() {
            let docs = get_collection_stats(name).0;
            let vecs = collection_vector_count(name);
            per_coll.push((name.clone(), docs, vecs));
        }
    }

    // Global fp + model diags for fingerprint status / model diagnostics (distinct counts
    // surface mixed chunker/model issues across the index; sample for health).
    // Queries are cheap + read-only; graceful 0 on any error (no table etc).
    let (fp_distinct, model_distinct, fp_sample) = if let Ok(conn) =
        crate::db::open_connection(true)
    {
        let fps: u32 = conn
            .query_row(
                "SELECT COUNT(DISTINCT embed_fingerprint) FROM content_vectors",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let mods: u32 = conn
            .query_row(
                "SELECT COUNT(DISTINCT model) FROM content_vectors",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let samp: String = conn
            .query_row(
                "SELECT COALESCE(embed_fingerprint, '') FROM content_vectors ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();
        (fps, mods, samp)
    } else {
        (0, 0, String::new())
    };

    // Vector store health note (BLOB storage path used by this port; no vec0 virtual table for storage).
    let vec_store_health = if vec_count > 0 {
        if fp_distinct > 1 {
            "present (note: multiple fingerprints — consider cleanup or re-embed after model/chunker change)"
        } else {
            "present (consistent)"
        }
    } else {
        "n/a"
    };

    // Warnings using embed info (graceful, no hard fails)
    let mut warnings: Vec<String> = Vec::new();
    if doc_count > 0 && vec_count == 0 && real_embed {
        warnings.push("documents present but no vectors — run update --embed".into());
    }
    if !real_embed && embed_m.is_some() {
        warnings
            .push("embed model configured but llama-embed feature inactive in this build".into());
    }
    if collection_count > 0 && doc_count == 0 {
        warnings.push("collections configured but empty index — run update".into());
    }
    if fp_distinct > 1 && real_embed {
        warnings.push("multiple embedding fingerprints detected — vectors may be from different models/chunkers (see status for details)".into());
    }

    if json {
        // Extended JSON (back-compat: original fields preserved; new keys added for #4).
        // Use serde_json::json! for the variable portions so that model paths and
        // warning texts containing quotes, backslashes, etc. are correctly escaped.
        // (Addresses raw-interpolation bug in the 0.5 slice.)
        let models_val = serde_json::json!({
            "embed": embed_m.as_deref().unwrap_or(""),
            "generate": gen_m.as_deref().unwrap_or(""),
            "rerank": rer_m.as_deref().unwrap_or("")
        });
        let editor_val = serde_json::json!(editor);
        let warn_val = serde_json::json!(warnings);
        let fp_val = serde_json::json!(fp_distinct);
        let model_val = serde_json::json!(model_distinct);
        let vec_store_val = serde_json::json!(vec_store_health);
        // Minimal per-coll for JSON consumers (name/docs/vecs); avoids large arrays.
        let per_coll_val = serde_json::json!(per_coll
            .iter()
            .map(|(n, d, v)| serde_json::json!({"name":n, "documents":d, "vectors":v}))
            .collect::<Vec<_>>());
        println!(
            r#"{{"version":"{}","rust":true,"exe":"{}","index":"{}","documents":{},"vectors":{},"collections":{},"embed_model":"{}","embed_dim":{},"real_embed":{},"vector_health":"{}","models":{},"editor_uri":{},"warnings":{},"embedding_fingerprints":{},"vector_models":{},"vector_store_health":{},"per_collection_embedding":{}}}"#,
            env!("CARGO_PKG_VERSION"),
            exe,
            index,
            doc_count,
            vec_count,
            collection_count,
            embed_model,
            embed_dim,
            real_embed,
            vec_health,
            models_val,
            editor_val,
            warn_val,
            fp_val,
            model_val,
            vec_store_val,
            per_coll_val
        );
    } else {
        println!("QMD Status (Rust port v{})", env!("CARGO_PKG_VERSION"));
        println!();
        println!("Binary: {}", exe);
        println!("Index:  {}", index);

        println!("Documents: {} ({} vectors)", doc_count, vec_count);
        println!("Collections: {}", collection_count);
        println!("Updated: {}", updated);

        // New in 0.5: embedding + health details
        println!();
        println!(
            "Embed model: {} (dim: {}, real: {})",
            embed_model, embed_dim, real_embed
        );
        if embed_m.is_some() || gen_m.is_some() || rer_m.is_some() {
            println!(
                "Models (config): embed={:?} generate={:?} rerank={:?}",
                embed_m, gen_m, rer_m
            );
        }
        // "Editor:" (human-friendly in TTY) vs "editor_uri" (machine key in JSON) is
        // intentional, matching the pattern for "Embed model" vs embed_model etc.
        println!(
            "Editor: {}",
            if editor.is_empty() {
                "not set (plain paths; set QMD_EDITOR_URI or editor_uri in config for clickable TTY links)"
            } else {
                &editor
            }
        );
        println!("Vector health: {}", vec_health);

        // #4 richer observability (text surface)
        println!("Embedding fingerprints (distinct): {}", fp_distinct);
        if model_distinct > 0 || !fp_sample.is_empty() {
            println!(
                "Vector models (distinct): {} (sample fp: {})",
                model_distinct,
                if fp_sample.is_empty() {
                    "n/a"
                } else {
                    &fp_sample
                }
            );
        }
        println!("Vector store health: {}", vec_store_health);

        if !per_coll.is_empty() {
            println!();
            println!("Per-collection embedding health (docs / vectors):");
            for (name, docs, vecs) in &per_coll {
                println!("  {}: {} / {}", name, docs, vecs);
            }
        }

        if !warnings.is_empty() {
            println!();
            println!("Warnings:");
            for w in &warnings {
                println!("  - {}", w);
            }
        }
    }
    Ok(())
}
