//! `qmd skill show/install` + `qmd skills list/get/path` (Iteration 1, smallest viable).
//!
//! Bundled skill assets are now embedded at compile time (include_str!) for full
//! distribution parity: `qmd skill` and `qmd skills` work from any installed binary
//! (cargo install, releases, etc.) with no runtime source tree or CARGO_MANIFEST_DIR dep.
//! install writes the bootstrap stub + references tree (original full SKILL.md replaced
//! per prior behavior). skills cmds delegate to embedded content. Follows per-command
//! module precedent exactly; no monolith in mod.rs.

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

use crate::cli::args::{SkillAction, SkillsAction};

/// Skill assets embedded at compile time from the source tree at build.
/// This is the fix for the pre-existing distribution bug: content ships inside
/// the binary for `cargo install` / dist users (dev `cargo run` gets updates on rebuild).
const BUNDLED_SKILL_MD: &str = include_str!("../../../skills/qmd/SKILL.md");
const BUNDLED_REFERENCES_MCP_SETUP: &str =
    include_str!("../../../skills/qmd/references/mcp-setup.md");

/// The minimal bootstrap stub written on install (tells agent to run `qmd skill show`).
fn installed_stub_content() -> &'static str {
    r#"---
name: qmd
description: Bootstrap QMD search instructions from the installed qmd CLI. Use when users ask to find notes, retrieve documents, inspect a wiki, or answer from indexed local markdown.
license: MIT
compatibility: Requires qmd CLI. Run `qmd skill show` for version-matched instructions.
allowed-tools: Bash(qmd:*), mcp__qmd__*
---

# QMD - Query Markdown Documents

This installed skill is intentionally a small bootstrap so it does not go stale
when the qmd package updates.

Load the full, version-matched QMD instructions from the CLI:

!`qmd skill show`

If your agent does not support bang-command expansion, run:

```
qmd skill show
```

Then follow those instructions. In short: search first, fetch full sources with
`qmd get` or `qmd multi-get`, and answer from retrieved text rather than snippets.
"#
}

/// Handle `qmd skill <show|install ...>`
pub fn cmd_skill(action: SkillAction) -> Result<()> {
    match action {
        SkillAction::Show { .. } => {
            // Use embedded content (compile-time include_str!): fixes the distribution
            // bug so `qmd skill show` works from any installed binary (no runtime
            // CARGO_MANIFEST_DIR or source tree required).
            println!("QMD Skill");
            println!();
            let content = if BUNDLED_SKILL_MD.ends_with('\n') {
                BUNDLED_SKILL_MD.to_string()
            } else {
                format!("{}\n", BUNDLED_SKILL_MD)
            };
            print!("{}", content);
        }
        SkillAction::Install { global, force, .. } => {
            // Target calculation unchanged (supports --global and local .agents).
            let home = std::env::var("HOME").unwrap_or_default();
            let target = if global {
                PathBuf::from(&home)
                    .join(".agents")
                    .join("skills")
                    .join("qmd")
            } else {
                std::env::current_dir()?
                    .join(".agents")
                    .join("skills")
                    .join("qmd")
            };

            if target.exists() {
                if !force {
                    eprintln!(
                        "Skill already exists: {} (use --force to replace)",
                        target.display()
                    );
                    std::process::exit(1);
                }
                let _ = fs::remove_dir_all(&target);
            }

            // Write embedded assets directly (no runtime copy from source dir).
            // This is the core of the dist bug fix: the references/ tree + stub are
            // always available from the binary itself.
            fs::create_dir_all(&target)?;
            fs::create_dir_all(target.join("references"))?;
            fs::write(target.join("SKILL.md"), installed_stub_content())?;
            fs::write(
                target.join("references").join("mcp-setup.md"),
                BUNDLED_REFERENCES_MCP_SETUP,
            )?;

            println!("✓ Installed QMD skill to {}", target.display());
            println!(
                "  (Run `qmd skill show` from anywhere in this tree to print full instructions.)"
            );
        }
    }
    Ok(())
}

/// Handle `qmd skills <list|get|path ...>` (thin delegation).
pub fn cmd_skills(action: Option<SkillsAction>) -> Result<()> {
    // "bundled" is now always the embedded content (post dist bug fix); no FS lookup.
    match action.unwrap_or(SkillsAction::List) {
        SkillsAction::List => {
            println!("  qmd  QMD agent skill (bundled)");
            println!("       (embedded in this binary; `qmd skill show` for content)");
        }
        SkillsAction::Get { name } => {
            if name != "qmd" {
                eprintln!("Unknown skill: {}", name);
                std::process::exit(1);
            }
            // Embedded full content (was previously read from dir).
            print!("{}", BUNDLED_SKILL_MD);
        }
        SkillsAction::Path { name } => {
            if name != "qmd" {
                eprintln!("Unknown skill: {}", name);
                std::process::exit(1);
            }
            // No real FS path for embedded; report clearly (avoids prior "not found" error).
            println!("(embedded in the qmd binary; view with `qmd skill show`)");
        }
    }
    Ok(())
}
