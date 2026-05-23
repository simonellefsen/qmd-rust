---
type: wiki-index
tags:
  - qmd-rust/wiki
  - maintained-by-llm
updated: 2026-05-23
---

# QMD-Rust Knowledge Wiki

This index is the content-oriented map for the LLM-maintained wiki of the qmd Rust port project. Agents should read this file (and `wiki/schema.md`) early when working on architecture, long-term plans, model integration, or wiki-specific features.

qmd (the tool) is the recommended search backend for exactly this kind of persistent, compounding LLM wiki. The Rust port exists to make that backend safe and fast when agents shell out to `qmd` or connect via MCP.

## Start Here

- [schema](schema.md) — Rules, page types, ingest/query/lint workflows, safety.
- [log](log.md) — Append-only maintenance timeline (parseable headings).
- [concepts/rust-for-python-node-developers](concepts/rust-for-python-node-developers.md) — Essential reading if you come from Python, Node.js or TypeScript (the Rust "newbie notes" page).
- [concepts/llm-wiki-for-search-tools](concepts/llm-wiki-for-search-tools.md) — Why qmd is the perfect meta-tool for LLM wikis.
- [concepts/rust-port-architecture](concepts/rust-port-architecture.md) — Current crate choices and layering (in progress).

## Source Notes

- [sources/llm-wiki](sources/llm-wiki.md) — Source-note for Andrej Karpathy's original LLM Wiki gist (the pattern this wiki follows).

## Concepts

- [concepts/llm-wiki-for-search-tools](concepts/llm-wiki-for-search-tools.md) — qmd as the retrieval primitive in the three-layer wiki architecture.
- [concepts/rust-for-python-node-developers](concepts/rust-for-python-node-developers.md) — Mental model shifts, common idioms, and practical explanations for Python/Node/TS developers reading the source (the official "Rust newbie notes").
- [concepts/rust-port-architecture](concepts/rust-port-architecture.md) — SQLite + FTS5 + vec0, llama.cpp bindings, tree-sitter, MCP, security model.
- [concepts/security-for-agent-tools](concepts/security-for-agent-tools.md) — Why a Rust CLI/MCP binary is preferable to Node for LLM shell/MCP usage.

## Runbooks

- [runbooks/rust-development](runbooks/rust-development.md) — Cargo workflow, testing against the Node reference binary, parity requirements, `cargo run --` vs installed `qmd`.
- [runbooks/model-management](runbooks/model-management.md) — GGUF embedding/rerank/expansion models, cache locations, CPU/GPU flags, updates.
- [runbooks/release](runbooks/release.md) — How the Rust binary will be packaged, Nix, potential Homebrew, changelog rules.

## Decisions

- [decisions/README](decisions/README.md) — Architecture decision records (crate selection, FFI boundaries, local vs global index, etc.).

## Experiments

- [experiments/README](experiments/README.md) — Crate evaluations, chunking strategies, vector backend alternatives, performance benchmarks.

## Open Questions (for this wiki and qmd itself)

- How should qmd expose wiki frontmatter (`type`, `tags`, `sources`, `updated`) in search results and MCP tools?
- Should `qmd` grow first-class "wiki mode" or "project knowledge" commands (e.g. `qmd wiki lint`, `qmd wiki ingest`)?
- Best way to keep a project-local `.qmd/` index inside the repo (uncommitted) vs named global collections for a growing wiki.
- Stable docid + citation format that works well when agents file new wiki pages back into the index.
- Integration points with Obsidian (graph, dataview on frontmatter) and future agent IDEs.

The structure follows the LLM Wiki pattern described in the root `llm-wiki.md` file and has been successfully used in multiple production projects that integrate qmd search with agent-driven knowledge maintenance.
