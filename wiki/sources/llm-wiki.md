---
type: source-note
tags:
  - qmd-rust/wiki
  - llm-wiki
  - karpathy
updated: 2026-05-23
source: https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f
author: Andrej Karpathy
source_url: https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f
---

# Source Note: LLM Wiki (Karpathy)

Source: [karpathy/442a6bf555914893e9891c11519de94f](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)

Credit: the original "LLM Wiki" idea file is by Andrej Karpathy. This is the canonical description of the three-layer pattern (raw sources → LLM-maintained wiki synthesis → schema/contract) that qmd is designed to support as the search/retrieval tool.

## Summary

The pattern replaces pure RAG (re-deriving answers from raw chunks on every query) with a persistent, compounding markdown wiki that the LLM incrementally builds and maintains. 

When new sources arrive:
- LLM reads them
- Extracts key information
- Updates entity/concept pages, resolves contradictions, strengthens synthesis
- Updates index.md and appends to log.md

The wiki becomes the "compiled" knowledge artifact. Cross-references, contradictions, and syntheses are already present instead of being recomputed.

Key files the LLM (and humans) use:
- `index.md` — content catalog (read this first)
- `log.md` — chronological, parseable history
- `schema.md` (or AGENTS.md / CLAUDE.md) — the contract

Optional but powerful tools:
- qmd (this project) for hybrid search over the wiki pages (BM25 + vector + rerank, CLI + MCP)
- Obsidian for graph view, backlinks, local graph, dataview queries over frontmatter

## Why qmd-rust exists in this context

qmd is called out in the original gist as a good choice for the search engine layer ("qmd is a good option: it's a local search engine for markdown files with hybrid BM25/vector search and LLM re-ranking, all on-device. It has both a CLI (so the LLM can shell out to it) and an MCP server").

The Rust port makes this recommendation even stronger:
- No Node.js supply-chain risk when an LLM agent repeatedly shells `qmd` or connects via stdio MCP.
- Fast cold starts (important for agent loops).
- Small static-ish binary with auditable FFI (SQLite + llama.cpp).
- Same on-device GGUF models for embeddings/reranking/expansion.

## Links

- Original gist and full text: [../llm-wiki.md](../llm-wiki.md) (kept at repo root for easy access)
- Concrete implementation example that follows this pattern: the `rust_daytrader` project (a real Rust codebase using qmd + Hermes + this wiki layout)
- Concepts in this wiki that build on it: [../concepts/llm-wiki-for-search-tools](../concepts/llm-wiki-for-search-tools.md)
