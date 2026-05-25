---
type: wiki-log
tags:
  - qmd-rust/wiki
  - maintained-by-llm
updated: 2026-05-25
---

# QMD-Rust Wiki Log

Append-only timeline of wiki maintenance. Headings use the format `## [YYYY-MM-DD] kind | summary` for easy parsing by agents and `grep`.

## [2026-05-25] release | v0.6.1 patch (version bump + annotated tag + push)

- Bumped version in Cargo.toml: 0.6.0 → 0.6.1.
- Restructured CHANGELOG: added dedicated `## [0.6.1] - 2026-05-25` section with the release hygiene fix; cleaned the prior long planning text under [Unreleased].
- Created annotated tag `v0.6.1` and pushed both the commit and the tag to origin.
- This patch release exists solely to deliver a working `dist host` / cargo-dist pipeline after the v0.6.0 workflow drift. No changes to qmd binary behavior or features.
- All AGENTS.md rules observed: wiki-first (this entry before the version edit), full fmt + clippy (default + llama-embed) before commit, smallest viable, only release files committed (large pending Iteration 2 src/ changes left uncommitted).
- `dist plan --tag=v0.6.1` will now succeed in CI for the new tag.

## [2026-05-25] release | fix cargo-dist "out of date contents" blocker for v0.6.0 (and future tags)

- Ran `dist generate` (after `git checkout -- dist-workspace.toml` to keep our rich Homebrew/tap/musl/publish-jobs config) to produce a stock `.github/workflows/release.yml` from cargo-dist 0.32.0.
- This exactly removed the 7-line "# Reproducibility / provenance notes..." block (plus 2 blanks) that the generator no longer emits and that caused the precise CI failure on `dist host --steps=create --tag=v0.6.0 --output-format=json`.
- Restored the yml to pure generated form; reproducibility notes already lived in `wiki/runbooks/release.md` (as documented there to avoid this exact class of drift).
- Verified: `dist plan --tag=v0.6.0` now succeeds without the error (only expected cross-compile warnings in the local env). `cargo fmt --all -- --check` and `cargo clippy -- -D warnings` (default + --features llama-embed) clean.
- Updated CHANGELOG under [Unreleased]. Small targeted hygiene patch only (release.yml + this log + changelog); the large v0.6.0 src changes remain uncommitted for separate handling.
- Per AGENTS.md: wiki-first update + checks before commit. Prepares for clean `git commit` of the fix + optional v0.6.1 patch tag.

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

## [2026-05-24] planning | Next parity roadmap after v0.5.1

- Created new decision record `decisions/2026-05-next-parity-phases.md` with a clear phased plan for remaining work toward full TS qmd parity.
- Updated `decisions/README.md` and `wiki/index.md` to reference the new roadmap.
- Plan breaks remaining work into small reviewable iterations:
  1. Surface completeness (`init`, `bench`, `skill show/install`, `skills` commands)
  2. Real LLM power (actual reranker model, better auto-expansion, `--chunk-strategy auto`)
  3. Agent experience polish (editor URI, deeper MCP, full skills packaging)
- Each future iteration will: update wiki (this log + relevant pages), implement in smallest viable slices with review loop, run fmt+clippy, update CHANGELOG, commit + tag + push.
- This keeps the LLM wiki itself as a living example of using qmd for agent-driven project knowledge.

## [2026-05-23] port | Rust qmd CLI skeleton + status parity

- Initialized Cargo bin `qmd`, basic clap subcommand surface matching the Node original.
- Implemented working `status` (reads `~/.config/qmd/index.yml` + queries the real SQLite for doc/vector counts) and version/help.
- Added rusqlite + serde_yaml; passes fmt + clippy -D warnings.
- AGENTS.md created (replaced legacy CLAUDE.md) with explicit llm-wiki integration section.
- Goal: make the Rust binary the preferred safe target for exactly the agentic wiki-maintenance flows documented in the daytrader example.

## [2026-05-24] ci | Harden integration test and enforce formatting for reliable releases

- Fixed `cargo fmt --all -- --check` failure that was occurring in GitHub Actions release workflows (the multiline `.unwrap()` on `execute_batch` was not matching rustfmt expectations in the released tree).
- Hardened `index::tests::test_update_path_end_to_end_with_ignore_patterns` so it bootstraps the minimal required schema (`content`, `documents`, `documents_fts`, `store_collections`) using `CREATE TABLE IF NOT EXISTS`. This prevents the "no such table: content" panic on fresh CI runners that have never run the Node version of qmd.
- Confirmed locally: `cargo fmt --all -- --check` passes cleanly and `cargo test --lib` (all 15 tests, including the previously flaky integration test) passes even against a completely empty SQLite file.
- This change ensures that future `cargo test --all` runs inside `cargo-dist` / release workflows will succeed on pristine GitHub runners.
- Part of the ongoing "on each iteration keep wiki up to date + commit + tag + push" discipline for release hygiene.

## [2026-05-24] iteration | Start Iteration 1 — Surface Completeness (target v0.5.2)

- Official start of Iteration 1 per `wiki/decisions/2026-05-next-parity-phases.md`.
- Wiki-first rule executed: this log entry (and optional roadmap note) added *before* touching any Rust source or creating new .rs command modules.
- Exact scope locked to smallest viable:
  1. `qmd init` (local .qmd/ + yml/sqlite; CLI prefers local when present, global fallback).
  2. `qmd bench <fixture.json>` (minimal JSON loader + metrics using existing search/query paths; no new deps).
  3. `qmd skill show` + `qmd skill install [--global]` (bundled via CARGO_MANIFEST_DIR + copy to ./.agents or ~/.agents; no claude symlink in smallest slice).
  4. `qmd skills list/get/path` (thin delegation to discovered skill location).
- Enforced: proper per-command modules (init.rs, bench.rs, skill.rs) exactly like context.rs / multi_get.rs; no logic dump in mod.rs.
- All constraints: zero outside-workspace path strings in code/comments/changelog; never execute mutating qmd; fmt+clippy after every slice + end; update CHANGELOG under [Unreleased]; leave clean tree for orchestrator's final commit/tag v0.5.2/push.
- Review loop (implement/fix via dedicated review notes file) until zero open issues.
- This keeps the LLM wiki as living example.

## [2026-05-24] iteration | Start Iteration 2 — Real LLM Power (target v0.6.0)

- Official start of Iteration 2 per `wiki/decisions/2026-05-next-parity-phases.md`. (I1 complete with v0.5.2.)
- Wiki-first rule executed: this log entry + roadmap decision record update added *before* any Rust source edits or new modules (per standing rule on each iteration).
- Exact scope locked to smallest viable high-impact slices only:
  1. Real reranker: load/use real model from `models.rerank` (or fallback) via existing llama-cpp-2/embedder path (no new gen scaffolding); integrate after fusion in query; respect --no-rerank / --candidate-limit / --explain; replaces heuristic with model-driven semantic cosine rerank on candidates for meaningful reordering on real data.
  2. Better automatic expansion: enhance current (intent + plain-text-as-vec) with multi-vector pseudo-HyDE style variants reusing embedder only (no LLM generate); more diverse vec signals fused via RRF.
  3. Wire `--chunk-strategy auto`: add arg to embed/update; extend chunking in index/ with std-only language skeleton (regex boundary markers for Rust + TS/JS etc at fn/class); graceful fallback to simple/regex for other files/failures; update fingerprint + embed pipeline to be strategy-aware.
- Enforced: proper architecture (reranker logic + embedder extension in src/embed/* (edited existing); chunk strategy extends src/index/mod.rs; no new .rs files created; no monoliths in query.rs); followed existing patterns exactly (e.g. EMBED_* consts, simple_chunk callers, LlamaEmbedder ctor style, RRF fuse).
- All constraints (non-negotiable): zero references to any paths/files outside the workspace in *new* code/comments/changelog; never run any mutating `qmd` commands; run `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings` (and with --features llama-embed) after every meaningful sub-slice + at very end; update CHANGELOG.md under ## [Unreleased]; at success (0-issue loop) leave tree clean for orchestrator's final wiki polish + changelog + commit + next-minor tag (v0.6.0) + push.
- Slices done with implement → (self) review via grep/read + fix loop until clean.
- This keeps the LLM wiki itself as a living, maintained example.

