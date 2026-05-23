---
type: wiki-log
tags:
  - qmd-rust/wiki
  - maintained-by-llm
updated: 2026-05-23
---

# QMD-Rust Wiki Log

Append-only timeline of wiki maintenance. Headings use the format `## [YYYY-MM-DD] kind | summary` for easy parsing by agents and `grep`.

## [2026-05-23] setup | LLM-wiki structure for qmd-rust

- Created `wiki/` directory following the LLM Wiki pattern (schema.md, index.md, parseable log.md, typed source-notes, concepts/, runbooks/, etc.).
- Added `schema.md`, `index.md`, `log.md`, `sources/`, `concepts/`, `runbooks/`, `decisions/`, `experiments/`.
- Adopted the structure (YAML frontmatter with `type`/`tags`/`sources`, relative links, parseable log entries, clear separation of raw sources vs. synthesized pages).
- Root `llm-wiki.md` remains at project root as the original idea file; a source-note version lives under `wiki/sources/`.

## [2026-05-23] inspiration | LLM Wiki pattern

- Studied real-world applications of the LLM Wiki pattern in production projects that combine qmd, agent tooling, and Obsidian.
- Key observations: top-level AGENTS.md (or equivalent) directs agents to read `wiki/index.md` first and follow `wiki/schema.md`; dedicated documentation covers qmd collection setup and Obsidian workflows.
- qmd is treated as a core part of the operating loop (search commands appear in runbooks and diagrams; agents are expected to use `qmd query` / MCP).
- Confirmed the value of consistent YAML frontmatter (`type`, `tags`, `updated`, `sources: []`), relative links, and parseable log entries.
- Noted practical questions around scaling qmd collections and frontmatter-aware search — directly relevant to qmd-rust development.

## [2026-05-23] port | Rust qmd CLI skeleton + status parity

- Initialized Cargo bin `qmd`, basic clap subcommand surface matching the Node original.
- Implemented working `status` (reads `~/.config/qmd/index.yml` + queries the real SQLite for doc/vector counts) and version/help.
- Added rusqlite + serde_yaml; passes fmt + clippy -D warnings.
- AGENTS.md created (replaced legacy CLAUDE.md) with explicit llm-wiki integration section.
- Goal: make the Rust binary the preferred safe target for exactly the agentic wiki-maintenance flows documented in the daytrader example.
