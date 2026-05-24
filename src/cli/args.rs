//! Clap argument definitions for the qmd CLI.
//!
//! These types define the entire command-line surface. They are intentionally
//! kept in one place so the help text and validation stay consistent.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Top-level CLI definition.
#[derive(Parser, Debug)]
#[command(
    name = "qmd",
    version,
    about = "QMD — Quick Markdown Search (Rust port)",
    long_about = "On-device hybrid search (BM25 + vector + LLM rerank) for your notes, docs, and wikis.\n\nRust implementation for security and performance. See AGENTS.md and llm-wiki.md.",
    after_help = "Use `qmd <command> --help` for command-specific options.\n\nWhile the Rust port is under development, the Node reference binary is available as `/opt/homebrew/bin/qmd` (or `qmd` in PATH from npm)."
)]
pub struct Cli {
    /// The subcommand the user typed (query, status, etc.).
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Use a named index (default: index)
    #[arg(long, global = true, default_value = "index")]
    pub index: String,
}

/// All possible top-level subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Hybrid search with auto expansion + reranking (recommended)
    Query {
        query: Vec<String>,
        #[arg(short = 'n', long, default_value_t = 5)]
        n: usize,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        min_score: Option<f32>,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
        #[arg(short = 'c', long)]
        collection: Option<String>,
        #[arg(long)]
        explain: bool,
        #[arg(long)]
        no_rerank: bool,
        /// Show full document content (instead of snippet)
        #[arg(long)]
        full: bool,
        /// Include line numbers with --full output
        #[arg(long)]
        line_numbers: bool,
    },

    /// Full-text BM25 keyword search (no LLM)
    Search {
        query: Vec<String>,
        #[arg(short = 'n', long, default_value_t = 5)]
        n: usize,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        min_score: Option<f32>,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
        #[arg(short = 'c', long)]
        collection: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        files: bool,
    },

    /// Vector similarity search only
    Vsearch {
        query: Vec<String>,
        #[arg(short = 'n', long, default_value_t = 5)]
        n: usize,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        min_score: Option<f32>,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
        #[arg(short = 'c', long)]
        collection: Option<String>,
        /// (future) Show full document
        #[arg(long)]
        full: bool,
        /// (future)
        #[arg(long)]
        line_numbers: bool,
    },

    /// Retrieve a document by path or docid (#abc123)
    Get {
        file: String,
        #[arg(short = 'l', long)]
        l: Option<usize>,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        line_numbers: bool,
    },

    /// Batch retrieve by glob or comma list
    #[command(name = "multi-get")]
    MultiGet {
        pattern: String,
        #[arg(short = 'l', long)]
        l: Option<usize>,
        #[arg(long)]
        max_bytes: Option<usize>,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
    },

    /// Manage collections (add/list/remove/rename)
    Collection {
        #[command(subcommand)]
        action: CollectionAction,
    },

    /// Attach human-written context/summaries to paths or collections
    Context {
        #[command(subcommand)]
        action: ContextAction,
    },

    /// List collections or files within a collection (supports qmd:// virtual paths)
    Ls { path: Option<String> },

    /// Show index health, collections, embedding status
    Status {
        #[arg(long)]
        json: bool,
    },

    /// Initialize a project-local .qmd index
    Init {
        #[arg(long)]
        force: bool,
    },

    /// Re-index collections (optionally git pull first)
    Update {
        #[arg(long)]
        pull: bool,
        /// After raw content update, also (re)generate embeddings for new or changed chunks.
        /// Requires a build with the `llama-embed` feature and a GGUF embed model configured
        /// (QMD_EMBED_MODEL or models.embed in index.yml). Uses fingerprinting to skip work.
        #[arg(long)]
        embed: bool,
    },

    /// Generate or refresh vector embeddings
    Embed {
        #[arg(short = 'f', long)]
        force: bool,
        #[arg(short = 'c', long)]
        collection: Option<String>,
    },

    /// Start the MCP server
    Mcp {
        #[arg(long)]
        http: bool,
        #[arg(long, default_value_t = 8181)]
        port: u16,
        #[arg(long)]
        daemon: bool,
    },

    /// Internal / diagnostic commands
    #[command(hide = true)]
    Bench { fixture: PathBuf },

    #[command(hide = true)]
    Skills {
        #[command(subcommand)]
        action: Option<SkillsAction>,
    },

    /// Clear caches, orphaned vectors/content, vacuum DB (maintenance)
    Cleanup,

    /// Show or install the bundled QMD agent skill (for Claude/Cursor/etc)
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum CollectionAction {
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        mask: Option<String>,
    },
    List,
    Remove {
        name: String,
    },
    Rename {
        old: String,
        new: String,
    },
    Show {
        name: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ContextAction {
    Add {
        path: Option<String>,
        text: Vec<String>,
    },
    List,
    Rm {
        path: String,
    },
    Check,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SkillsAction {
    List,
    Get { name: String },
    Path { name: String },
}

#[derive(Subcommand, Debug, Clone)]
pub enum SkillAction {
    Show {
        // Fields accepted for future extension / global option compatibility (from clap parse).
        // TS singular `qmd skill show` (qmd.ts:4415) ignores them (only plural `skills` uses --full/--json);
        // harmless to advertise; impl in Area 3 will ignore for exact singular-path fidelity.
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
    },
    Install {
        #[arg(long)]
        global: bool,
        #[arg(short = 'f', long)]
        force: bool,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Csv,
    Md,
    Xml,
    Files,
}
