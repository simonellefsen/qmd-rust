---
type: decision
tags:
  - qmd-rust
  - roadmap
  - parity
updated: 2026-05-24
---

# Decision: Next Parity Phases After v0.5.1 (Toward Full TS qmd Parity)

## Context
After v0.5.1 (context commands, heuristic reranker + auto-expansion in query, cleanup, improved status, multi-get, proper module extraction, test hardening), the advertised CLI surface in `--help` is largely wired.

However, several commands remain in the "not yet implemented" catch-all or are only partially real:
- `init`
- `bench`
- `skill show/install` + `skills` subcommands
- Real (model-driven) reranker (current is heuristic only)
- `--chunk-strategy auto` (tree-sitter AST chunking)
- Full editor URI support (`QMD_EDITOR_URI`)
- Deeper MCP tool parity and skills packaging

The original TypeScript qmd has a very complete surface for agent use (structured queries with real expansion/rerank, skills for self-instruction, bench for quality, etc.).

## Decision
We will execute the remaining parity work in small, reviewable iterations. Each iteration will:
- Update this wiki (new decision record or update to this page + log.md + index.md)
- Land working, tested code in smallest viable slices
- Update CHANGELOG under [Unreleased]
- Commit + tag (patch or minor as appropriate) + push

## Proposed Iteration Plan (as of 2026-05-24)

### Iteration 1 — Surface Completeness (v0.5.2 / v0.6.0 prep)
Goal: Remove the last items from the catch-all and make advertised commands actually do something useful.

- Finish `qmd init` (project-local `.qmd/` index with its own sqlite + yml)
- Implement `qmd bench <fixture>` (basic harness that exercises query/search against a JSON fixture and reports simple metrics)
- Implement `qmd skill show` + `qmd skill install` (basic discovery + copy of the bundled skill into `.agents/skills/qmd` or global)
- Wire `qmd skills list/get/path` (thin delegation to the installed skill location)

### Iteration 2 — Real LLM Power (High Impact)
Goal: Make `qmd query` deliver on its "recommended + auto expansion + reranking" promise with actual models.

- Load and use a real reranker model (`models.rerank` in config, GGUF via llama-cpp)
- Improve automatic expansion (beyond current text-as-vec)
- Wire `--chunk-strategy auto` (at least basic tree-sitter integration for Rust/TS/Python; fall back gracefully)

### Iteration 3 — Agent Experience Polish
- Full `QMD_EDITOR_URI` clickable output in TTY
- Deeper MCP tool surface (more tools, better structured results)
- Complete skills packaging and `skills` subcommands
- Richer `status` + `cleanup` (model health, embedding fingerprints, etc.)
- AST chunking as the default for code files

## Trade-offs & Constraints
- Every slice must remain small and reviewable (no monoliths, proper per-command modules).
- Prioritize agent/MCP/llm-wiki use cases (the original motivation).
- Keep the "never auto-run mutating qmd commands" and "print examples only" rule.
- Update wiki on every iteration before or with the code commit.

## Next Action
User will select the first slice from Iteration 1 (or adjust priorities). Work will proceed with wiki update → implementation in review loop → commit + tag + push.

This decision record will be updated as iterations complete.