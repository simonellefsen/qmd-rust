//! CLI layer (argument parsing and command implementations)

pub mod args;

// Re-export the main types so callers can do `use qmd::cli::args::Cli;`
pub use args::{Cli, Commands, OutputFormat};
