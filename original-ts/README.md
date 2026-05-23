# Original TypeScript Implementation (Archived)

This directory contains the original Node.js / TypeScript / Bun implementation of QMD that this Rust port is based on.

It is kept here purely for reference while the Rust version reaches feature parity (exact FTS5 query semantics, collection handling, MCP tool surface, output formats, etc.).

**Do not modify files in this directory** unless you are explicitly porting a specific piece of behavior and need the original as the source of truth.

The active Rust implementation lives at the repository root (`src/main.rs`, `Cargo.toml`, etc.).

See the root [README.md](../README.md) and [AGENTS.md](../AGENTS.md) for the current state of the Rust port.