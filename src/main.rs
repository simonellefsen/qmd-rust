//! QMD - Query Markup Documents (Rust port)
//!
//! Secure, fast, on-device hybrid search for markdown and knowledge bases.
//! Drop-in replacement for the Node.js original with better security profile.
//!
//! # For Rust newbies (Python / Node.js / TypeScript background)
//!
//! This file is the equivalent of:
//! - `src/cli/qmd.ts` (the main CLI entry point) in the original project, **plus**
//! - the "bin" launcher logic.
//!
//! Key Rust concepts explained inline with comments:
//! - `struct` + `enum` with `#[derive(Parser)]` from the `clap` crate
//!   replaces `argparse.ArgumentParser` (Python) or `yargs`/`commander` (Node).
//!   The derive macro generates all the parsing + `--help` for free.
//! - `Option<T>` is Rust's version of `T | null` / `Optional[T]` / `T | undefined`.
//! - `Result<T, E>` + the `?` operator is how Rust does error handling without
//!   exceptions. Think of it as "explicit try/catch that the compiler enforces".
//!   We use `anyhow::Result` because it is the most ergonomic choice for CLI tools
//!   (it can hold any error and still gives nice `.context()` messages).
//! - `&str` vs `String`: `&str` is a cheap borrowed view (like a slice), `String`
//!   owns heap-allocated data. Pass `&str` to functions when you don't need ownership.
//! - Pattern matching (`match`, `if let Some(x)`) is one of Rust's superpowers.
//!   It is exhaustive and prevents the "forgot a case" bugs common in other languages.
//!
//! The long-term goal is to port the real logic from `src/store.ts`, `src/db.ts`
//! and `src/llm.ts` into well-commented Rust modules. Start here, then move to
//! separate files (`src/commands/status.rs`, `src/db.rs`, etc.) as the port grows.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const INDEX_PATH: &str = "~/.cache/qmd/index.sqlite";
const CONFIG_DIR: &str = "~/.config/qmd";

/// Top-level CLI definition.
///
/// `#[derive(Parser)]` tells the `clap` crate to automatically generate a parser,
/// version handling, and beautiful `--help` output from the struct definition.
/// This is the Rust equivalent of defining an ArgumentParser + subparsers in Python
/// or using `yargs.command()` / `commander` in Node.js, but with full type safety.
///
/// The fields with `#[arg(...)]` or `#[command(...)]` become the actual CLI flags
/// and subcommands. Clap even validates types at compile time (you can't pass
/// a string where a number is expected, for example).
#[derive(Parser, Debug)]
#[command(
    name = "qmd",
    version = VERSION,
    about = "QMD — Quick Markdown Search (Rust port)",
    long_about = "On-device hybrid search (BM25 + vector + LLM rerank) for your notes, docs, and wikis.\n\nRust implementation for security and performance. See AGENTS.md and llm-wiki.md.",
    after_help = "Use `qmd <command> --help` for command-specific options.\n\nWhile the Rust port is under development, the Node reference binary is available as `/opt/homebrew/bin/qmd` (or `qmd` in PATH from npm)."
)]
struct Cli {
    /// The subcommand the user typed (query, status, etc.).
    /// Wrapped in `Option` because the user might just run `qmd` with no subcommand.
    #[command(subcommand)]
    command: Option<Commands>,

    /// Use a named index (default: index)
    /// This is a *global* flag — it appears on every subcommand.
    #[arg(long, global = true, default_value = "index")]
    index: String,
}

/// All possible top-level subcommands.
///
/// In Rust, an `enum` (algebraic data type) is much more powerful than a
/// TypeScript union or a Python string literal + if/elif chain.
/// Each variant can carry its own strongly-typed data (the fields inside `{ ... }`).
///
/// `#[derive(Subcommand)]` from clap turns every variant into a real CLI subcommand
/// with its own help text, arguments, and validation.
///
/// This is the heart of the CLI "state machine". In `main()` we `match` on it
/// (exhaustive pattern matching — the compiler will complain if we forget a case).
#[derive(Subcommand, Debug, Clone)]
enum Commands {
    /// Hybrid search with auto expansion + reranking (recommended)
    Query {
        /// The query text or structured document (lex:/vec:/hyde:)
        query: Vec<String>,

        /// Max results
        #[arg(short = 'n', long, default_value_t = 5)]
        n: usize,

        /// Output all matches (pair with --min-score)
        #[arg(long)]
        all: bool,

        /// Minimum score threshold
        #[arg(long)]
        min_score: Option<f32>,

        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,

        /// Filter to specific collection(s)
        #[arg(short = 'c', long)]
        collection: Option<String>,

        /// Include retrieval score traces
        #[arg(long)]
        explain: bool,

        /// Skip LLM reranking
        #[arg(long)]
        no_rerank: bool,
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
    },

    /// Retrieve a document by path or docid (#abc123)
    Get {
        file: String,
        /// Max lines
        #[arg(short = 'l', long)]
        l: Option<usize>,
        /// Show full document
        #[arg(long)]
        full: bool,
        /// Add line numbers
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
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// Initialize a project-local .qmd index (instead of global ~/.cache/qmd)
    Init {
        /// Force overwrite of existing local index
        #[arg(long)]
        force: bool,
    },

    /// Re-index collections (optionally git pull first)
    Update {
        /// Run git pull in each collection first
        #[arg(long)]
        pull: bool,
    },

    /// Generate or refresh vector embeddings
    Embed {
        /// Force re-embed everything
        #[arg(short = 'f', long)]
        force: bool,
        /// Limit to one collection
        #[arg(short = 'c', long)]
        collection: Option<String>,
    },

    /// Start the MCP server (stdio for agents, or --http)
    Mcp {
        /// Serve over HTTP instead of stdio
        #[arg(long)]
        http: bool,
        /// Port for HTTP transport
        #[arg(long, default_value_t = 8181)]
        port: u16,
        /// Run as background daemon (HTTP only)
        #[arg(long)]
        daemon: bool,
    },

    /// Internal / diagnostic commands (bench, skills, etc.)
    #[command(hide = true)]
    Bench { fixture: PathBuf },

    /// Placeholder for other commands during port
    #[command(hide = true)]
    Skills {
        #[command(subcommand)]
        action: Option<SkillsAction>,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum CollectionAction {
    /// Add/index a new collection
    Add {
        path: PathBuf,
        /// Explicit name for the collection
        #[arg(long)]
        name: Option<String>,
        /// Glob mask (default **/*.md)
        #[arg(long)]
        mask: Option<String>,
    },
    /// List all collections with details
    List,
    /// Remove a collection by name
    Remove { name: String },
    /// Rename a collection
    Rename { old: String, new: String },
    /// Show collection details
    Show { name: String },
}

#[derive(Subcommand, Debug, Clone)]
enum ContextAction {
    Add {
        /// Path (defaults to current dir, supports qmd://)
        path: Option<String>,
        /// The context text
        text: Vec<String>,
    },
    List,
    /// Remove context for a path
    Rm {
        path: String,
    },
    /// Check for collections/paths missing context
    Check,
}

#[derive(Subcommand, Debug, Clone)]
enum SkillsAction {
    List,
    Get { name: String },
    Path { name: String },
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
enum OutputFormat {
    #[default]
    Text,
    Json,
    Csv,
    Md,
    Xml,
    Files,
}

/// The program entry point (like `if __name__ == "__main__":` in Python or
/// the body of `bin/qmd` + the default export in the original TS CLI).
///
/// `main` must return `Result<(), anyhow::Error>` (or `()`) so we can use the `?`
/// operator for clean error propagation. If any function using `?` fails, the
/// error bubbles up and `anyhow` prints a nice message + backtrace (if RUST_BACKTRACE=1).
///
/// `Cli::parse()` is generated by clap at compile time. It reads `std::env::args()`
/// (the equivalent of `process.argv` in Node or `sys.argv` in Python) and turns
/// it into the strongly-typed `Cli` struct we defined above.
fn main() -> Result<()> {
    let cli = Cli::parse();

    // Rust's `match` is exhaustive and extremely powerful.
    // It is the primary way we "dispatch" on which subcommand the user asked for.
    // Because `Commands` is an enum, the compiler guarantees we handle every possible case.
    match cli.command {
        None => {
            // No subcommand — print friendly help
            println!("qmd (Rust port) v{} — use --help for usage", VERSION);
            println!(
                "Primary: qmd query <q> | search <q> | vsearch <q> | get <file> | status | mcp"
            );
            println!("Reference Node binary still available for full functionality during port: /opt/homebrew/bin/qmd");
        }

        Some(Commands::Status { json }) => {
            cmd_status(json)?;
        }

        Some(Commands::Init { force }) => {
            eprintln!("`qmd init` (project-local index) is not yet implemented in Rust.");
            eprintln!("Reference: /opt/homebrew/bin/qmd init");
            eprintln!("(Creates .qmd/ with local index.sqlite for this directory's wiki.)");
            if force {
                eprintln!("(force flag noted)");
            }
        }

        // This arm catches every command we have *declared* in the enum but have
        // not finished porting yet. The `..` means "I don't care about the fields".
        //
        // During the port this is the polite way to tell the user (or an LLM agent)
        // "please use the mature Node version for this operation for now".
        // Once a command is implemented, you move its arm *above* this catch-all.
        Some(Commands::Query { .. })
        | Some(Commands::Search { .. })
        | Some(Commands::Vsearch { .. })
        | Some(Commands::Get { .. })
        | Some(Commands::MultiGet { .. })
        | Some(Commands::Ls { .. })
        | Some(Commands::Update { .. })
        | Some(Commands::Embed { .. })
        | Some(Commands::Mcp { .. })
        | Some(Commands::Collection { .. })
        | Some(Commands::Context { .. })
        | Some(Commands::Bench { .. })
        | Some(Commands::Skills { .. }) => {
            eprintln!("This command is not yet implemented in the Rust port.");
            eprintln!("During development, use the reference implementation:");
            eprintln!(
                "  /opt/homebrew/bin/qmd {}",
                std::env::args().nth(1).unwrap_or_default()
            );
            eprintln!(
                "\nSee AGENTS.md for porting status and how to contribute to the Rust version."
            );
            std::process::exit(2);
        }
    }

    Ok(())
}

/// Handle the `qmd status` (and `status --json`) command.
///
/// This is currently the most "real" command in the Rust port — it actually
/// talks to the same files the Node version uses (`~/.config/qmd/index.yml`
/// and the SQLite database at `~/.cache/qmd/index.sqlite`).
///
/// The `?` operator here means: "if `load_config()` returns an `Err`, return
/// that error from this function immediately". This is the idiomatic Rust
/// replacement for Python's `try: ... except: raise` or Node's `.catch()`.
fn cmd_status(json: bool) -> Result<()> {
    let index = expand_tilde(INDEX_PATH);
    let _config_dir = expand_tilde(CONFIG_DIR);

    // `unwrap_or_default()` gives us an empty config instead of crashing if
    // the YAML file is missing or unreadable. Good for a "best effort" status.
    let cfg = load_config().unwrap_or_default();

    // These two helpers return `Option<...>`. The `?` inside them turns a
    // failed DB open into `None`, which we then turn into (0, 0) or "unknown".
    let (doc_count, vec_count) = db_counts(INDEX_PATH).unwrap_or((0, 0));
    let updated = last_updated_hint(INDEX_PATH).unwrap_or_else(|| "unknown".to_string());

    let collection_count = cfg.collections.as_ref().map(|c| c.len()).unwrap_or(0);

    if json {
        let collections_json = cfg
            .collections
            .as_ref()
            .map(|cols| {
                cols.iter()
                    .map(|(k, v)| {
                        format!(
                            r#"{{ "name": "{}", "path": "{}", "pattern": "{}" }}"#,
                            k, v.path, v.pattern
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        println!(
            r#"{{"version":"{}","rust":true,"index":"{}","documents":{}, "vectors":{}, "collections":{}, "collections_detail":[{}]}}"#,
            VERSION, index, doc_count, vec_count, collection_count, collections_json
        );
    } else {
        println!("QMD Status (Rust port v{})", VERSION);
        println!();
        println!("Index: {}", index);
        if let Ok(meta) = fs::metadata(expand_tilde(INDEX_PATH)) {
            let size_mb = meta.len() as f64 / 1_048_576.0;
            println!("Size:  {:.1} MB", size_mb);
        }
        println!();
        println!("Documents");
        println!("  Total:    {} files indexed", doc_count);
        println!("  Vectors:  {} embedded", vec_count);
        println!("  Updated:  {}", updated);
        println!();
        println!("Collections ({})", collection_count);
        if let Some(cols) = &cfg.collections {
            for (name, c) in cols {
                println!("  {} (qmd://{}/)", name, name);
                println!("    Path:     {}", c.path);
                println!("    Pattern:  {}", c.pattern);
            }
        } else {
            println!("  (none configured — run the Node qmd or ported collection add)");
        }
        println!();
        if let Some(m) = &cfg.models {
            println!("Models");
            if let Some(e) = &m.embed {
                println!("  Embedding:   {}", e);
            }
            if let Some(r) = &m.rerank {
                println!("  Reranking:   {}", r);
            }
            if let Some(g) = &m.generate {
                println!("  Generation:  {}", g);
            }
        }
        println!();
        println!("Tip: Rust qmd is a safe CLI/MCP target for LLM agents (llm-wiki.md).");
        println!(
            "     Full search/embed/MCP parity is the current development focus — see AGENTS.md."
        );
    }
    Ok(())
}

/// Expand `~/foo` into `/home/user/foo` (or equivalent on the platform).
///
/// `std::env::var_os("HOME")` returns an `OsString` (platform-native string).
/// `.to_string_lossy()` turns it into a `Cow<str>` that is cheap to use in `format!`.
///
/// This helper exists because the original Node code and the user's config files
/// use the familiar `~` shortcut. In a real application you would usually use the
/// `dirs` or `home` crate, but a tiny manual version is fine for teaching.
fn expand_tilde(p: &str) -> String {
    if let Some(home) = env::var_os("HOME") {
        if let Some(stripped) = p.strip_prefix("~/") {
            return format!("{}/{}", home.to_string_lossy(), stripped);
        }
    }
    p.to_string()
}

// -----------------------------------------------------------------------------
// Config (index.yml) + partial DB status for `qmd status`
// -----------------------------------------------------------------------------
//
// These structs + the functions below demonstrate several important Rust idioms
// that will appear everywhere once we port the real store/db logic:
//
// - `#[derive(Deserialize)]` + serde = automatic deserialization (like Pydantic
//   `BaseModel` or a Zod schema). The field names must match the YAML keys
//   (or you use `#[serde(rename = "...")]`).
// - `Option<T>` for "this key might be missing".
// - `HashMap<K, V>` is Rust's hash table (very close to Python `dict` or JS object/Map).
// - Helper functions that return `Result` or `Option` for fallible operations.
// - The `?` operator + `.with_context(...)` from anyhow for rich error messages.

/// The top-level shape of `~/.config/qmd/index.yml`.
/// Collections and model overrides live here (the DB is the runtime cache).
#[derive(Debug, Deserialize, Default)]
struct QmdConfig {
    /// Map from collection name → its configuration.
    /// Using `HashMap` because the YAML is a mapping under the `collections:` key.
    collections: Option<std::collections::HashMap<String, CollectionCfg>>,
    models: Option<ModelsCfg>,
}

/// One entry under `collections:` in the YAML.
/// Example:
///   mynotes:
///     path: /home/user/notes
///     pattern: "**/*.md"
#[derive(Debug, Deserialize)]
struct CollectionCfg {
    path: String,
    /// We provide a default so the YAML doesn't have to repeat the common case.
    #[serde(default = "default_pattern")]
    pattern: String,
}

/// Used by serde's `default` attribute above.
fn default_pattern() -> String {
    "**/*.md".to_string()
}

#[derive(Debug, Deserialize, Default)]
struct ModelsCfg {
    embed: Option<String>,
    generate: Option<String>,
    rerank: Option<String>,
}

/// Read and parse the user's `~/.config/qmd/index.yml`.
///
/// Returns a default (empty) config if the file does not exist — this is
/// intentional so `qmd status` never hard-fails just because the user hasn't
/// created any collections yet.
///
/// The `?` + `.with_context(...)` pattern is the Rust way to add human-readable
/// context to errors without losing the original cause (very useful when an
/// agent or user sees the error message).
fn load_config() -> Result<QmdConfig> {
    let path = PathBuf::from(expand_tilde("~/.config/qmd/index.yml"));
    if !path.exists() {
        return Ok(QmdConfig::default());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let cfg: QmdConfig = serde_yaml::from_str(&text)
        .with_context(|| format!("failed to parse YAML at {}", path.display()))?;
    Ok(cfg)
}

/// Try to open the SQLite index read-only and return (document_count, vector_count).
///
/// Returns `None` on any failure (file missing, sqlite-vec not loadable yet, etc.).
/// This is the "best effort" style you will see a lot in CLI tools.
///
/// Key Rust/SQLite idioms here:
/// - `Connection::open_with_flags(..., SQLITE_OPEN_READ_ONLY)` — we never want
///   to accidentally modify the user's index while just showing status.
/// - `.ok()?` — convert a `Result<T,E>` into `Option<T>`. If it was an error,
///   we early-return `None` from the whole function (the `?` on Option).
/// - `query_row(sql, params, |row| { ... })` — the closure receives a `Row`
///   and you extract columns by index or name. Very similar to `cursor.fetchone()`
///   in Python's sqlite3 or `db.get()` in better-sqlite3, but the closure style
///   makes it hard to misuse the row after the query finishes.
/// - `unwrap_or(0)` — if the count query somehow fails, treat it as zero instead
///   of crashing the status command.
fn db_counts(db_path: &str) -> Option<(u32, u32)> {
    let expanded = expand_tilde(db_path);
    let conn =
        Connection::open_with_flags(&expanded, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let doc_count: u32 = conn
        .query_row("SELECT COUNT(*) FROM documents WHERE active=1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let vec_count: u32 = conn
        .query_row("SELECT COUNT(*) FROM content_vectors", [], |r| r.get(0))
        .unwrap_or(0);
    Some((doc_count, vec_count))
}

/// Same idea as `db_counts`, but we only want the most recent `modified_at` timestamp
/// so the status output can say "Updated: ...".
///
/// `COALESCE` is SQLite's `COALESCE` / `IFNULL` (returns the first non-null value).
/// `unwrap_or_default()` on a `String` gives you `""`.
fn last_updated_hint(db_path: &str) -> Option<String> {
    let expanded = expand_tilde(db_path);
    let conn =
        Connection::open_with_flags(&expanded, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let ts: String = conn
        .query_row(
            "SELECT COALESCE(MAX(modified_at), '') FROM documents WHERE active=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if ts.is_empty() {
        None
    } else {
        Some(ts)
    }
}
