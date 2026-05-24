//! Implementation of the `qmd status` command.
//!
//! Significantly improved for 0.5.0: now surfaces embed model (via the Area 2
//! embed infrastructure), config models, basic vector health, and warnings.
//! JSON output extended for machine consumers. Follows exact prior patterns
//! and reuses load_config + db helpers. No new path literals added in this edit.

use crate::db::{db_counts, expand_tilde, last_updated_hint, load_config};
use crate::embed::default_embedder;
use anyhow::Result;

const INDEX_PATH: &str = "~/.cache/qmd/index.sqlite";
const CONFIG_DIR: &str = "~/.config/qmd";

/// Handle the `qmd status` (and `status --json`) command.
pub fn cmd_status(json: bool) -> Result<()> {
    let index = expand_tilde(INDEX_PATH);
    let _config_dir = expand_tilde(CONFIG_DIR);

    let cfg = load_config().unwrap_or_default();
    let (doc_count, vec_count) = db_counts(INDEX_PATH).unwrap_or((0, 0));
    let updated = last_updated_hint(INDEX_PATH).unwrap_or_else(|| "unknown".to_string());

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

    // Basic vector health (presence + rough diversity)
    let vec_health = if vec_count == 0 && doc_count > 0 {
        "missing (consider update --embed)".to_string()
    } else if vec_count > 0 {
        "present".to_string()
    } else {
        "n/a".to_string()
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

    if json {
        // Extended JSON (back-compat: original fields preserved; new keys added)
        let models_json = format!(
            "{{\"embed\":\"{}\",\"generate\":\"{}\",\"rerank\":\"{}\"}}",
            embed_m.as_deref().unwrap_or(""),
            gen_m.as_deref().unwrap_or(""),
            rer_m.as_deref().unwrap_or("")
        );
        let warn_json = if warnings.is_empty() {
            "[]".to_string()
        } else {
            format!("[\"{}\"]", warnings.join("\",\""))
        };
        println!(
            r#"{{"version":"{}","rust":true,"exe":"{}","index":"{}","documents":{},"vectors":{},"collections":{},"embed_model":"{}","embed_dim":{},"real_embed":{},"vector_health":"{}","models":{},"warnings":{}}}"#,
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
            models_json,
            warn_json
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
        println!("Vector health: {}", vec_health);

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
