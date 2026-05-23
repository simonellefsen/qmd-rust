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

- Created `wiki/` directory following the concrete pattern from the `rust_daytrader` project (a sibling repository's llm-maintained wiki).
- Added `schema.md`, `index.md`, `log.md`, `sources/`, `concepts/`, `runbooks/`, `decisions/`, `experiments/`.
- Copied/adapted the structure (frontmatter types, relative links, parseable log, source-notes, separation of raw vs. synthesized).
- Recorded the exploration of the `rust_daytrader` project's wiki as the primary real-world inspiration for applying the Karpathy LLM Wiki pattern inside the qmd-rust codebase.
- Root `llm-wiki.md` remains at project root as the original idea file; a source-note version will live under `wiki/sources/`.

## [2026-05-23] inspiration | rust_daytrader/wiki example

- Deep exploration of the working llm-wiki implementation in the sibling rust daytrader project.
- Observed: top-level AGENTS.md points agents to `wiki/index.md` + `wiki/schema.md`; `docs/project-wiki.md` documents qmd collection setup, Obsidian, and workflows.
- qmd is explicitly part of the operating loop (search in diagrams, `rtk qmd query/search` commands in runbooks and schema).
- Confirmed heavy use of YAML frontmatter (`type`, `tags`, `updated`, `sources: []`), relative links, parseable log entries, and clear raw-vs-wiki boundaries.
- Noted open questions in that wiki about scaling qmd collections for larger wikis — directly relevant to qmd-rust feature work.

## [2026-05-23] port | Rust qmd CLI skeleton + status parity

- Initialized Cargo bin `qmd`, basic clap subcommand surface matching the Node original.
- Implemented working `status` (reads `~/.config/qmd/index.yml` + queries the real SQLite for doc/vector counts) and version/help.
- Added rusqlite + serde_yaml; passes fmt + clippy -D warnings.
- AGENTS.md created (replaced legacy CLAUDE.md) with explicit llm-wiki integration section.
- Goal: make the Rust binary the preferred safe target for exactly the agentic wiki-maintenance flows documented in the daytrader example.
