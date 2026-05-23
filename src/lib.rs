//! QMD - Query Markup Documents (Rust port)
//!
//! This crate contains the core logic for the qmd CLI tool.
//! The binary entrypoint is in `src/main.rs` (thin dispatcher).
//!
//! Public API is centralized here: re-exports for args, db, commands, etc.
//! This makes the library easy for contributors and for embedding (e.g. tests, future MCP server).
//! Contributor pattern: 1) add variant in cli/args.rs 2) impl pub fn cmd_* in cli/commands/new.rs
//! 3) declare in commands/mod.rs 4) wire dispatch in main.rs 5) (opt) re-export here.

pub mod cli;
pub mod config;
pub mod db;
// pub mod mcp;   // to be extracted later (MCP logic currently lives under cli::commands::mcp)
// pub mod utils;

pub use cli::args::{Cli, Commands, OutputFormat};
pub use cli::commands; // command handlers (cmd_status, cmd_search, etc.) for dispatch and reuse
pub use db::{
    db_counts, expand_tilde, get_collection_stats, last_updated_hint, load_config,
    load_config_value, open_connection, save_config_value,
    search::{build_fts5_query, fts_search, FtsHit},
    CollectionCfg, ModelsCfg, QmdConfig,
};
