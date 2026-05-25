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
pub mod embed; // Area 2: embedding generation (on top of update)
pub mod index; // Area 2: file discovery + indexing (update/embed)
               // pub mod mcp;   // to be extracted later (MCP logic currently lives under cli::commands::mcp)
pub mod utils;

pub use cli::args::{
    ChunkStrategy, Cli, CollectionAction, Commands, ContextAction, OutputFormat, SkillAction,
    SkillsAction,
};
pub use cli::commands; // command handlers (cmd_status, cmd_search, etc.) for dispatch and reuse
pub use db::{
    active_config_path, active_db_path, build_editor_uri, db_counts, editor_uri, expand_tilde,
    format_path_for_output, get_collection_stats, last_updated_hint, load_config,
    load_config_value, open_connection, resolve_document_fs_path, save_config_value,
    search::{build_fts5_query, fts_search, vec_search, FtsHit},
    stdout_is_tty, CollectionCfg, ModelsCfg, QmdConfig,
};
