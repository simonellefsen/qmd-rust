# QMD-Rust — Query Markup Documents (Rust port)

Secure, high-performance Rust reimplementation of the QMD on-device hybrid search engine for markdown, code, and knowledge bases.

**Goal**: Provide a drop-in, security-hardened replacement for the original Node.js/TypeScript [qmd](https://github.com/tobi/qmd) that eliminates supply-chain risks associated with large JS runtimes and native modules while preserving (and improving) functionality, speed, and local LLM integration.

QMD powers agentic workflows and is the recommended search backend in Andrej Karpathy's [llm-wiki pattern](llm-wiki.md) — the persistent, LLM-maintained wiki that sits between raw sources and queries. Rust qmd gives LLM agents a trustworthy `qmd` CLI and MCP tool they can shell to or connect without worrying about Node.js CVEs.

## Key Commands (Rust version)

Use `cargo run --` during development (or `./target/debug/qmd` / `cargo install --path .` once built):

```sh
cargo run -- --help
cargo run -- collection list
cargo run -- status
cargo run -- query "your search" --json
cargo run -- mcp
```

Once installed as `qmd-rust` (or symlinked as `qmd` in a test env), the CLI surface matches the original:

- `qmd query` (hybrid + expand + rerank — recommended)
- `qmd search` (BM25/FTS5)
- `qmd vsearch` (vector only)
- `qmd get`, `qmd multi-get`
- `qmd collection add/list/remove/rename`
- `qmd context add/list/rm/check`
- `qmd embed`, `qmd update`, `qmd status`, `qmd cleanup`
- `qmd mcp [--http]`
- `qmd ls`, `qmd init`, `qmd bench`

See the README, docs/SYNTAX.md, and original source (src/ tree) for command semantics and query grammar. The Rust port aims for full flag/argument/JSON/CSV/MD output parity.

## Development

- **Tooling**: Rust 1.85+, Cargo, `rustup`, `cargo clippy`, `cargo fmt`, `cargo test`.
- **Run the Rust binary**: `cargo run -- <command> [args...]`
- **Compare with Node reference**: The host has the original `qmd` (Node/Bun) installed at `/opt/homebrew/bin/qmd`. Always verify behavioral parity on commands you implement:
  ```sh
  /opt/homebrew/bin/qmd status
  cargo run -- status
  # Diff JSON outputs, snippet formatting, error messages, etc.
  ```
- **Build**: `cargo build --release` (produces `target/release/qmd`).
- **Testing**: `cargo test`. Port relevant tests from `test/` (vitest) to Rust tests. Keep the original TS tests green during transition.
- **DB**: The index lives at `~/.cache/qmd/index.sqlite` (same as Node version). Rust **must not** corrupt it. Use the exact same schema (documents, content, content_vectors, documents_fts FTS5, vectors_vec via sqlite-vec vec0, llm_cache, store_* tables).
- **Never auto-run mutating commands** (`collection add`, `embed`, `update`, `context add`) against a user's real index during agent work unless explicitly asked and the user has a backup. Document example commands for the human to run.
- **Changelog**: Add entries under `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md) for every user-visible change, bugfix, or architectural decision. Follow the same release standards as the original (see skills/release/).
- **Formatting**: `cargo fmt`. Run `cargo clippy -- -D warnings` before committing.
- **Comments for Rust newbies**: The primary audience for early comments is someone who knows Python/Node/TypeScript but is new to Rust. Add short, high-signal explanations next to:
  - `main()` and the big `match` on `Commands`
  - Every `#[derive(Parser/Subcommand)]` block (explain it replaces argparse/yargs)
  - `Option<T>`, `Result<T,E>`, the `?` operator, `if let Some(x)`
  - `&str` vs `String`, `HashMap`, closures passed to rusqlite, `.ok()?` / `unwrap_or` patterns
  - Any FFI or ownership nuance that would surprise a JS/Python developer.
  Keep comments concise (2-4 lines) and link back to AGENTS.md or a good Rust book when needed. The goal is "I can read this file and understand the shape even if I don't know every keyword yet."

## Architecture (Rust targets)

- **Persistence**: `rusqlite` + `libsqlite3-sys` (or `sqlite` crate) with FTS5. Load `sqlite-vec` extension for `vec0` virtual tables (cosine distance).
- **Embeddings / Reranking / Query expansion**: GGUF models via a safe llama.cpp binding (`llama-cpp-rs` or `llama-cpp-2` preferred for Metal/GPU on macOS + CPU fallback; evaluate `candle` + GGUF for pure-Rust option). Target the same defaults as the TS version (embeddinggemma, Qwen3 variants). Support CPU-only via env var.
- **Chunking**: Regex for Markdown + tree-sitter (`tree-sitter`, `tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-go`, `tree-sitter-typescript`) for AST-aware chunking on code files when `--chunk-strategy auto`.
- **Retrieval**: BM25 via FTS5, vector via vec0, Reciprocal Rank Fusion (RRF), optional LLM reranker. Structured queries (`lex:`, `vec:`, `hyde:`, `intent:`) and automatic expansion.
- **CLI**: `clap` (derive) with full subcommand parity, output formats (text, --json, --csv, --md, --xml, --files), color, line numbers, snippets.
- **MCP server**: stdio transport (primary for agents) + optional HTTP. Implement the same tool surface (`query`, `get`, `multi_get`, `status`, ...). Use `serde`, `tokio` if needed. Consider `rust-mcp-sdk` or hand-rolled JSON-RPC if lighter.
- **Context & collections**: Collections and human-written path contexts are stored in `~/.config/qmd/index.yml` (YAML) + mirrored in DB for self-containment. Rust must read/write the same format (use `serde_yaml` or `yaml-rust2`).
- **Security**: Static-ish binary where possible, minimal attack surface, no arbitrary code execution from indexed files, careful with model downloads and FFI. Prefer crates with good security track records and recent audits.
- **Performance**: Fast cold start (critical when LLM agents shell out `qmd` for every lookup). Lazy model loading, connection pooling for SQLite, sensible defaults for batch sizes.

See original `src/store.ts`, `src/db.ts`, `src/llm.ts`, `src/cli/qmd.ts` and `docs/SYNTAX.md` for exact semantics, query grammar, and edge cases to replicate.

## Project Knowledge Wiki (llm-wiki pattern)

This repository follows the LLM-maintained wiki pattern (raw sources → synthesized interlinked markdown pages → schema/contract) described in llm-wiki.md. The `wiki/` directory in this repo is the living knowledge base for the qmd-rust port.

- Read `wiki/index.md` **first** for any durable project knowledge (architecture, port status, model choices, why certain decisions were made).
- Follow `wiki/schema.md` when creating or updating wiki pages.
- Append every maintenance action to `wiki/log.md` using the parseable `## [YYYY-MM-DD] action | summary` format.
- Keep raw sources (original TS code, papers, gists, benchmark results) immutable; synthesize in `wiki/sources/`, `concepts/`, etc.
- Use relative links inside `wiki/` and absolute/repo-relative links for code outside it.
- The wiki is the *human + agent readable* synthesis layer. The actual source of truth for behavior remains the Rust code, tests, Cargo.toml, and the original TS sources (for parity).

See also:
- `wiki/schema.md` (detailed rules, page types, workflows, safety)
- `docs/project-wiki.md` in the daytrader example (qmd + Obsidian setup reference)
- Root `llm-wiki.md` (the original Karpathy idea file)

## Incorporating the LLM-Wiki Pattern (llm-wiki.md)

QMD exists to serve LLM-wikis. The Rust port makes the "search engine" layer trustworthy for exactly the use case described:

- **Raw sources** live in user collections (markdown, transcripts, papers, code).
- **The wiki** (LLM-written interlinked pages, index.md, log.md) is queryable via `qmd query` / MCP.
- **The schema** (this AGENTS.md + any project-specific instructions) tells the agent how to maintain the wiki and how to use qmd as its tool.

When you (the agent) work on this repo or help users build wikis:

1. Treat qmd as the **persistent retrieval primitive** the wiki LLM calls via shell (`qmd search --json`) or MCP. Design every CLI/MCP improvement with "will an LLM agent be able to rely on this reliably in a loop?" in mind.
2. For the qmd-rust source itself, you may maintain a small development wiki under `docs/wiki/` (or similar) following the three-layer pattern if it helps long-term knowledge accumulation across sessions. Keep raw research notes separate from synthesized pages. Update `index.md` and `log.md` via the LLM rather than hand-editing.
3. Prioritize features that reduce bookkeeping for wiki maintainers: excellent citation/docid support, full-document retrieval, context inheritance, fast structured queries, MCP tools that return exactly what an agent needs to synthesize or update wiki pages.
4. Lint passes, contradiction detection, and orphan-page suggestions are higher-level wiki operations — qmd can expose the raw material; the agent (or future `qmd wiki` subcommands) performs the synthesis.

Read [llm-wiki.md](llm-wiki.md) fully at the start of any session that involves knowledge-base or agentic features.

## Important Safety Rules (inherited + Rust-specific)

- **Never** execute `cargo run -- collection add ...`, `embed`, or `update` against a real user collection unless the user has explicitly asked you to demonstrate and has a known-good backup of `~/.cache/qmd/index.sqlite` and `~/.config/qmd/`.
- Write example commands for the human to run manually.
- Do not modify the SQLite DB schema or data directly with external tools.
- When porting, keep the Node `qmd` working until Rust reaches parity on a command.
- Model files are large; respect `~/.cache/qmd/models/` or the standard llama.cpp cache. Support `QMD_FORCE_CPU=1` / `--no-gpu`.
- FFI boundaries (sqlite-vec, llama.cpp) are the main security surface — keep them minimal and well-audited.

## Releasing the Rust Port

- Version parity with the TS releases initially (track in Cargo.toml).
- Use the existing release skill/process, but the binary artifact changes (no more Bun/Node packaging).
- The shell wrapper `bin/qmd` will eventually become a thin Rust launcher or be replaced by a pure Rust binary.
- Update Nix flake, Homebrew formula, etc., once the Rust build is solid.
- Full test matrix (including against the original TS outputs for regression) before cutting releases.
- **Mandatory pre-tag verification**: Before any `vX.Y.Z` annotated tag or push, run `./scripts/verify-release.sh` from the repo root (see the Pre-tag / Pre-push verification checklist (mandatory) immediately below and `wiki/runbooks/release.md` for the full reinforced gates, dist plan requirement, and root-cause diagnosis of prior v0.6.7–v0.6.11 incidents).

## Pre-tag / Pre-push verification checklist (mandatory)

Before creating or pushing *any* `vX.Y.Z` annotated tag (or release-related branch), the developer or orchestrator **must** run the verification script from the repository root and confirm success:

```sh
./scripts/verify-release.sh
```

The script executes the reinforced gates (fmt + both clippy variants + test --all, as formalized in this checklist and documented in the release runbooks):

```
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo clippy --features llama-embed -- -D warnings
cargo test --all
```

plus `cargo dist plan` (must succeed and announce artifacts for the version parsed from the *local* `Cargo.toml` at the current commit).

- If any gate or the dist plan fails, the script exits non-zero with actionable guidance (typically: bump the version in `Cargo.toml`, commit, and re-run).
- Only on the success banner ("All gates passed for vX.Y.Z from local Cargo.toml manifest. Safe to create annotated tag vX.Y.Z and push.") is it safe to proceed with `git tag -a vX.Y.Z -m "..."` followed by `git push origin main --tags`.
- Paste the complete script output into the release commit message or GitHub Release notes.

This enforces version alignment: `cargo dist` (plan + CI release jobs) uses the `version` field from the manifest at the tagged commit to decide what to build and publish. Tags created without a matching manifest bump produce the "doesn't have anything for dist to Release" error.

See `wiki/runbooks/release.md` (the "Pre-tag verification script (mandatory)" subsection) for the documented root cause of the v0.6.7–v0.6.11 incidents and full hygiene notes.

## Quick Reference — Original Node Commands (for parity testing)

(Kept for reference — see `qmd --help` from the installed Node binary or the help text in the Rust implementation.)

Use the installed `/opt/homebrew/bin/qmd` to capture expected outputs while implementing the Rust equivalent.

---

This file (AGENTS.md) is the living specification for agents working on qmd-rust. Evolve it as the architecture, crate choices, and wiki-integration features solidify. When in doubt, re-read llm-wiki.md and ask: "Does this make the wiki experience better for the human + LLM pair?"
