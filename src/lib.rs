//! QMD - Query Markup Documents (Rust port)
//!
//! This crate contains the core logic for the qmd CLI tool.
//! The binary entrypoint is in `src/main.rs`.

pub mod cli;
pub mod config;
pub mod db;
// pub mod mcp;   // to be extracted later
// pub mod utils;

pub use cli::args::{Cli, Commands, OutputFormat};
pub use db::{
    db_counts, expand_tilde, get_collection_stats, last_updated_hint, load_config,
    load_config_value, open_connection, save_config_value,
    search::{build_fts5_query, fts_search, FtsHit},
    CollectionCfg, ModelsCfg, QmdConfig,
};
