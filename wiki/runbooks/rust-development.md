---
type: runbook
tags:
  - qmd-rust/wiki
  - development
  - rust
updated: 2026-05-23
---

# Rust Development Runbook for qmd

This runbook captures the day-to-day commands, parity testing, and safety rules while the Rust port is in progress.

## Daily Commands

```bash
# Build & check
cargo check
cargo fmt -- --check
cargo clippy -- -D warnings

# Run the local binary (never shadows global qmd during dev)
cargo run -- --help
cargo run -- status
cargo run -- status --json

# Compare against the installed Node reference (always available)
qmd status          # or /opt/homebrew/bin/qmd status
qmd --help

# Test a specific subcommand once implemented
cargo run -- query "sqlite vec" --json -n 3
```

## Parity Requirements

- Output formats (`--json`, `--csv`, `--md`, `--files`, `--xml`) must be byte-for-byte or semantically identical for the same inputs where possible.
- Docids must match (first 6 chars of content hash).
- Error messages and exit codes should be close; the Rust version can be stricter/friendlier.
- Collection and context behavior must read/write the same `~/.config/qmd/index.yml` and DB tables.
- Never auto-run `collection add`, `embed`, `update`, or `context add` in agent sessions — only print the exact command for the human.

## Project-Local Index (`qmd init`)

The Node version supports `qmd init` to create a `.qmd/` directory with a local `index.sqlite` (useful inside a git repo so the wiki is searchable without a global collection).

The Rust port must support the same (and mark `.qmd/` in `.gitignore`).

## Adding a New Command

1. Add the variant to the `Commands` enum in `src/main.rs`.
2. Implement the handler (or "not yet implemented" stub that prints the reference binary command).
3. Update help text and AGENTS.md if the command has new semantics.
4. Add a matching entry in `wiki/log.md` and, if durable, a concept/decision page.
5. Test output against `qmd <same command>` from the Node binary.

## Testing Against a Real Wiki

Real-world LLM wikis (with frontmatter, typed pages, and cross-links) provide the best test corpus for exercising search, context inheritance, and large result sets while developing qmd. Your own indexed collections are the ideal corpus.

Use it (via the Node qmd today, via Rust qmd once `collection list` / search work) to validate that qmd is a first-class citizen for the exact use case it was designed to support.

## Safety

- The binary produced by `cargo build --release` is what will eventually be shipped to agents.
- All FFI (rusqlite, sqlite-vec extension, llama-cpp) must be minimal and reviewed.
- Model downloads go through the standard llama.cpp / hf cache mechanisms — no custom downloaders that could be confused for supply-chain attacks.

## Current Crate Layout (May 2026)

We are moving away from the original monolithic `src/main.rs` (which grew to ~1680 lines during the initial CLI implementation).

Target structure:

```
src/
├── main.rs          # thin: just parse + dispatch
├── lib.rs
├── cli/
│   ├── mod.rs
│   └── args.rs      # clap definitions (Cli, Commands, CollectionAction, ...)
├── config/          # YAML index.yml handling
├── db/              # rusqlite + FTS5 + collection persistence
├── mcp/             # MCP server implementation
└── utils/
```

**Rule of thumb for contributors**: New logic should go into the appropriate module under `src/`, not be appended to `main.rs`.

See `src/lib.rs` and the subdirectories for the current state of the refactor.

## Refactor Progress (as of this session)

- CLI args fully extracted to `src/cli/args.rs`
- DB layer (`src/db/mod.rs`) owns config + connection + collection stats helpers
- Search/FTS5 logic moved to `src/db/search.rs` (all sanitizers, `build_fts5_query`, `fts_search`)
- First command implementation moved: `cmd_status` → `src/cli/commands/status.rs`
- `src/lib.rs` + module re-exports in place for clean `use qmd::...` from main.rs

The goal is a thin `main.rs` that only does argument parsing and dispatch.

**Major win this session**: The entire FTS5/BM25 search engine (all sanitizers, CJK handling, `build_fts5_query`, `fts_search` with collection CTE) has been extracted into `src/db/search.rs`. `cmd_search` now delegates to it cleanly. The monolithic file is shrinking fast.
