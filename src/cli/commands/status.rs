//! Implementation of the `qmd status` command.

use crate::db::{db_counts, expand_tilde, last_updated_hint, load_config};
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

    if json {
        println!(
            r#"{{"version":"{}","rust":true,"exe":"{}","index":"{}","documents":{},"vectors":{},"collections":{}}}"#,
            env!("CARGO_PKG_VERSION"),
            exe,
            index,
            doc_count,
            vec_count,
            collection_count
        );
    } else {
        println!("QMD Status (Rust port v{})", env!("CARGO_PKG_VERSION"));
        println!();
        println!("Binary: {}", exe);
        println!("Index:  {}", index);

        // Size, collections, models, etc. can be expanded here later
        println!("Documents: {} ({} vectors)", doc_count, vec_count);
        println!("Collections: {}", collection_count);
        println!("Updated: {}", updated);
    }
    Ok(())
}
