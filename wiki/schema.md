---
type: wiki-schema
tags:
  - qmd-rust/wiki
  - maintained-by-llm
  - search-tool
updated: 2026-05-23
---

# QMD-Rust Wiki Schema

This file is the operating contract for agents (Codex, future Hermes, etc.) maintaining the qmd knowledge wiki.

qmd-rust is the secure Rust implementation of the search engine recommended in the LLM Wiki pattern. The wiki here documents the port, architecture decisions, model integration, and how qmd itself enables better LLM-maintained wikis.

## Layer Rules

- Raw sources (papers, gists, original TS code, issue discussions, model cards) are immutable. Summarize and cite; never rewrite as primary content.
- The `wiki/` tree is LLM-generated synthesis. Create/update pages here.
- The schema (this file + root AGENTS.md) defines conventions. Evolve it as the port and wiki workflows mature.
- Project-local indexes (via `qmd init`) or named collections are for search; the wiki pages are the human + agent readable layer.

## Page Types (YAML frontmatter)

Use consistent frontmatter on wiki pages:

```yaml
---
type: concept | runbook | decision | experiment | source-note | wiki-index | wiki-log | wiki-schema
tags:
  - qmd-rust/wiki
  - rust-port
updated: 2026-05-23
sources:
  - wiki/sources/llm-wiki.md
  - path/to/original/ts/file.ts
  - https://...
---
```

Recommended types:
- `wiki-index`, `wiki-log`, `wiki-schema`
- `source-note`
- `concept` (architecture, security model, retrieval strategies)
- `runbook` (development, testing parity, embedding management, release)
- `decision` (crate choices, FFI boundaries, model loading strategy)
- `experiment` (different chunkers, llama bindings vs candle, vector index alternatives)
- `capability` (new MCP tool, new query syntax feature)

## Link Rules

- Relative Markdown links inside `wiki/` (e.g. `[Rust Port Architecture](concepts/rust-port-architecture.md)`).
- Absolute paths or repo-relative for code outside `wiki/` and for the original TS sources in `src/`.
- Prefer linking to source files over large excerpts.
- Cite `docid`s from qmd search results when referencing indexed documents.
- Keep links Obsidian- and GitHub-friendly.

## Ingest Workflow

When a new source (paper, gist, TS implementation detail, user feedback, benchmark) matters:

1. Place or reference the raw source (copy key gists into `wiki/sources/` as needed).
2. Create or update a `wiki/sources/<name>.md` source-note page (with attribution, summary, original URL).
3. Update or create relevant concept / decision / runbook / experiment pages.
4. Update `wiki/index.md` (add links, one-line summaries).
5. Append a parseable entry to `wiki/log.md` (`## [YYYY-MM-DD] ingest | Title`).
6. If the insight affects the Rust implementation or qmd's fitness for llm-wikis, cross-link to code or open a decision.

## Query Workflow (for agents working on qmd-rust)

1. Read `wiki/index.md` first.
2. Use the local qmd (once `cargo run -- query ...` or installed binary supports collections or project-local index) or the reference `qmd` for search.
3. Retrieve full pages (`get`) before synthesizing.
4. Cite wiki pages + original sources + docids.
5. File any durable conclusion back into the appropriate wiki page + log it.

Example (once Rust port has collection support):
```bash
cargo run -- query "sqlite-vec loading" --json
cargo run -- get wiki/concepts/rust-port-architecture.md --full
```

## Lint Workflow

Periodically (or on request):

- `wiki/index.md` is complete and points to all maintained pages.
- `wiki/log.md` has entries for all maintenance.
- No contradictions between wiki pages and current Rust code / Cargo.toml / AGENTS.md.
- Missing concepts that are referenced repeatedly in code or issues get their page.
- Orphan pages and unresolved links are cleaned.
- Security notes (FFI, model download, secret handling) are current.
- qmd features that would improve wiki maintenance (better frontmatter facets, stable citations, fast MCP for agents) are tracked in decisions/experiments.

Useful local commands:
```bash
# Once Rust qmd supports it
cargo run -- status
cargo run -- ls wiki
cargo run -- search "reranker" -c qmd-rust-wiki
```

## Safety & Security Rules (critical for a search tool used by agents)

- Never put real user index paths, tokens, API keys, or model download tokens in wiki pages.
- Document security properties of chosen crates (rusqlite, llama-cpp-rs, etc.) and known risks of FFI.
- Model files and caches must be referenced by env/config, never hardcoded secrets.
- When describing MCP or CLI usage for LLMs, emphasize the reduced attack surface of the Rust binary vs Node.
- Wiki must not become an execution surface; it is read-only synthesis for reasoning.

## Relationship to Root AGENTS.md and llm-wiki.md

- Root [AGENTS.md](../AGENTS.md) is the coding contract for this session / Codex.
- This `wiki/schema.md` is the *knowledge wiki* contract.
- The original idea is in [../llm-wiki.md](../llm-wiki.md) and archived as source-note in `sources/`.
- The concrete working example that inspired this structure is the `rust_daytrader` project (a real-world Rust application that uses qmd + the exact same wiki layout with Hermes integration, multiple qmd collections, and Obsidian). Its wiki lives alongside the code in that repository.

Update this schema when the port or the wiki maintenance process evolves.
