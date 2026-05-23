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
