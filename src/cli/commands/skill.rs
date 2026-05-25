//! `qmd skill show/install` + `qmd skills list/get/path` (Iteration 1, smallest viable).
//!
//! Bundled skill discovery via CARGO_MANIFEST_DIR (works for cargo run / cargo install --path).
//! install copies the bundled tree + writes the bootstrap stub (no claude symlink in this slice).
//! skills commands are thin delegation to the (bundled or "installed") location.
//! Follows per-command module precedent exactly; no monolith in mod.rs.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::args::{SkillAction, SkillsAction};

/// Locate the bundled qmd skill dir from build-time manifest (no runtime outside refs).
fn bundled_skill_dir() -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR is a compile-time env var; the string in source is relative only.
    let root = env!("CARGO_MANIFEST_DIR");
    let p = Path::new(root).join("skills").join("qmd");
    if p.join("SKILL.md").exists() {
        Some(p)
    } else {
        None
    }
}

/// Read the primary SKILL.md content from a skill dir.
fn read_skill_md(dir: &Path) -> Result<String> {
    let p = dir.join("SKILL.md");
    Ok(fs::read_to_string(p)?)
}

/// Recursive copy of directory contents (files + subdirs). Mirrors original copyDirectoryContents.
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let s = entry.path();
        let d = dst.join(entry.file_name());
        if s.is_dir() {
            copy_dir_contents(&s, &d)?;
        } else if s.is_file() {
            if let Some(parent) = d.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::copy(&s, &d)?;
        }
    }
    Ok(())
}

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
            let dir = match bundled_skill_dir() {
                Some(d) => d,
                None => bail!("QMD skill not found (run from source tree or reinstall)"),
            };
            println!("QMD Skill");
            println!();
            let content = read_skill_md(&dir)?;
            print!(
                "{}",
                if content.ends_with('\n') {
                    content
                } else {
                    format!("{}\n", content)
                }
            );
        }
        SkillAction::Install { global, force, .. } => {
            let src = match bundled_skill_dir() {
                Some(d) => d,
                None => bail!("QMD skill not found for install"),
            };

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

            copy_dir_contents(&src, &target)?;

            // Write the bootstrap stub (replaces the full one per original behavior).
            fs::write(target.join("SKILL.md"), installed_stub_content())?;

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
    let bundled = bundled_skill_dir();
    let effective = bundled.clone(); // for v0.5.2: only bundled (install location tracked by FS, not registry)

    match action.unwrap_or(SkillsAction::List) {
        SkillsAction::List => {
            if let Some(dir) = &effective {
                println!("  qmd  QMD agent skill (bundled)");
                println!("       {}", dir.display());
            } else {
                println!("No skills found");
            }
        }
        SkillsAction::Get { name } => {
            if name != "qmd" {
                eprintln!("Unknown skill: {}", name);
                std::process::exit(1);
            }
            if let Some(dir) = &effective {
                let content = read_skill_md(dir)?;
                print!("{}", content);
            } else {
                eprintln!("Skill not found");
                std::process::exit(1);
            }
        }
        SkillsAction::Path { name } => {
            if name != "qmd" {
                eprintln!("Unknown skill: {}", name);
                std::process::exit(1);
            }
            if let Some(dir) = &effective {
                println!("{}", dir.display());
            } else {
                eprintln!("Skill not found");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
