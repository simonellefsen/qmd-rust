# QMD-Rust — Query Markup Documents

**Secure, on-device search for your knowledge bases — now in Rust.**

QMD indexes markdown, notes, meeting transcripts, documentation, and code. It offers fast hybrid search (BM25 full-text + vector similarity) with optional LLM re-ranking and query expansion, all running locally.

This is a from-scratch Rust reimplementation of the original [qmd](https://github.com/tobi/qmd) tool. The primary motivation is **security and minimal trusted computing base** when the tool is used by LLM agents (via CLI or MCP).

## Quick Start

```sh
# Build from source (recommended while in active development)
git clone https://github.com/simonellefsen/qmd-rust.git
cd qmd-rust
cargo build --release
./target/release/qmd --help
```

Or run directly during development:

```sh
cargo run -- status
cargo run -- search "your query"
```

### Basic Usage

```sh
# Create a collection
qmd collection add ~/notes --name notes

# Add human-written context (very powerful for agents)
qmd context add qmd://notes "Personal notes and research"

# Search
qmd search "project timeline"
qmd query "quarterly planning process"     # recommended (expansion + reranking when available)

# Retrieve documents
qmd get "notes/2025-01-15.md"
qmd get "#abc123"                          # by docid shown in search results

# List what you have
qmd ls
qmd ls notes/2025
```

See the [wiki/](wiki/) for detailed usage, the [original SYNTAX.md](docs/SYNTAX.md) for the query grammar, and the [Rust newcomer notes](wiki/concepts/rust-for-python-node-developers.md) if you are coming from Python/Node/TS.

## MCP Server

QMD can run as an MCP server so agents and IDEs can use it natively:

```sh
qmd mcp                    # stdio (most common)
qmd mcp --http --port 8181 # HTTP transport
```

The Rust implementation currently exposes the core tools (`status`, `get`, lexical `query`, `multi_get`). Full hybrid search and embedding support will be added in follow-up work.

## Project Status & Philosophy

- The core CLI (collections, `ls`, `get`, lexical `search`, `status`, basic MCP) is implemented and usable.
- This port deliberately stays close to the original semantics while being a clean Rust codebase.
- LLM/embedding features (`query` with expansion + reranking, `embed`, `vsearch`) are intentionally stubbed for now; they will be added using `llama-cpp-rs` (or an equivalent safe GGUF runtime) in a later phase.
- The project dogfoods its own [llm-wiki pattern](wiki/) — see `wiki/`, `AGENTS.md`, and `llm-wiki.md`.

## Development

```sh
cargo run -- <command>     # e.g. cargo run -- search "foo" --json
cargo fmt
cargo clippy -- -D warnings
cargo test                 # (once we have Rust-native tests)
```

All active development instructions live in [AGENTS.md](AGENTS.md) and the [wiki/](wiki/).

## Relationship to the Original

This repository began as a fork of the original TypeScript implementation. The old sources have been archived under `original-ts/` for reference during the port. The active codebase is pure Rust and aims for a much smaller, auditable attack surface.

## License

MIT — same as the original.

---

**Built for agents that need to remember things reliably and safely.**