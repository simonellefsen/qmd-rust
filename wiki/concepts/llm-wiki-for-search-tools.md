---
type: concept
tags:
  - qmd-rust/wiki
  - llm-wiki
  - agent-tools
updated: 2026-05-23
sources:
  - wiki/sources/llm-wiki.md
  - https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f
  - Real-world project wikis that apply the same three-layer pattern (raw sources, synthesized pages, schema)
---

# qmd as the Search Backend for LLM-Maintained Wikis

qmd (and especially the Rust port) is the natural "query" tool in the three-layer LLM Wiki architecture described by Karpathy.

## The Three Layers (recap)

1. **Raw sources** — immutable (code, papers, transcripts, gists, broker exports, design docs). The wiki never rewrites them.
2. **The wiki** (`wiki/`) — LLM-written, interlinked, maintained synthesis (concepts, runbooks, decisions, experiments, index, log).
3. **The schema/contract** (`wiki/schema.md` + root `AGENTS.md`) — tells the agent *how* to maintain the wiki and *how* to use tools like qmd.

## Where qmd Fits

- **Ingest** — After the LLM reads a new source and updates several wiki pages, it (or the human) can run `qmd update` + `qmd embed` (or the Rust equivalent) so future queries see the new content.
- **Query** — The primary way an agent "reads the wiki" at scale: `qmd query "..." --json`, `qmd get docid-or-path`, or via the MCP `query`/`get`/`multi_get` tools. Much better than raw `cat` or naive glob because of hybrid search + reranking + context.
- **Lint** — `qmd search` helps find orphan concepts, stale claims, or pages that should be cross-linked.

In the daytrader example, diagrams explicitly show "qmd search" as an arrow from the wiki layer, and runbooks contain `rtk qmd query ...` commands.

## Why the Rust Port Matters for This Use Case

When an LLM agent (Hermes, Codex, future systems) is in a tight loop maintaining a wiki, it will invoke the search tool dozens or hundreds of times per session:

- Shelling out to a Node.js `qmd` carries the full Node + native-module attack surface on every call.
- A small Rust binary compiled from auditable crates has a dramatically smaller trusted computing base.
- Cold-start time matters; `cargo run --` or a stripped `target/release/qmd` is fast.
- MCP stdio transport is a first-class citizen (no extra JS shim).

The existence of real projects that wire qmd into agent + wiki + Obsidian workflows validates the design and gives us excellent test data.

## Features qmd Should Grow to Better Serve Wikis

- First-class frontmatter awareness (return `type`, `tags`, `sources`, `updated` in JSON/MCP results; support filters like `type:concept`).
- Stable, short docids that survive re-indexing (already the 6-char hash prefix — keep them reliable).
- Project-local index support (`qmd init`) so a repo can ship its wiki with a ready-to-search `.qmd/` dir (ignored in git).
- MCP tools that return "suggested next pages to read" or "index excerpt + top hits".
- Citation-friendly output (title + path + docid + snippet + score).

These are tracked in [../decisions/README](../decisions/README.md) and experiments.

## Related

- [schema](../schema.md)
- [sources/llm-wiki](../sources/llm-wiki.md)
- Real-world reference implementations that combine qmd search with the LLM Wiki pattern.
- Root [llm-wiki.md](../../llm-wiki.md)
