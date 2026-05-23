---
type: concept
tags:
  - qmd-rust/wiki
  - rust
  - learning
  - python
  - nodejs
  - typescript
updated: 2026-05-23
---

# Rust for Python / Node.js / TypeScript Developers

This page collects the most important mental model shifts and "aha" moments when moving from dynamic languages (Python, JavaScript, TypeScript) into Rust, specifically in the context of building a CLI tool like qmd.

It is intentionally written for experienced developers who are new to Rust. The goal is to make the code in `src/main.rs` (and future modules) readable even before you are fluent in Rust.

## Core Mental Model Differences

| Concept              | Python / JS / TS                          | Rust                                                                 | Why it matters for qmd-rust |
|----------------------|-------------------------------------------|----------------------------------------------------------------------|-----------------------------|
| **Ownership**        | Garbage collector or reference counting   | Every value has exactly one owner. When the owner goes out of scope the value is dropped. | No surprise memory bugs in long-running CLI or MCP server. |
| **Borrowing**        | You can pass objects around freely        | You can borrow (`&T` or `&mut T`) but the compiler enforces rules at compile time. | Explains why we use `&str` a lot instead of cloning `String`. |
| **Error handling**   | Exceptions (`raise`, `throw`)             | `Result<T, E>` + `?` operator. Errors are values, not control flow. | `main() -> Result<()>` + `anyhow` is our version of "just let it crash with a good message". |
| **Null**             | `None`, `null`, `undefined`               | `Option<T>` (`Some(x)` or `None`). No null pointer dereference.     | `Option<String>` instead of `string \| undefined`. |
| **Types**            | Gradual / dynamic                         | Everything is checked at compile time. No `any`.                     | The clap derive macros give us type-safe CLI parsing for free. |
| **Enums**            | Mostly "labels" or string unions          | Algebraic Data Types — each variant can carry different data. Exhaustive `match`. | Our entire CLI (`Commands`, `CollectionAction`, etc.) is modeled as one big enum. |

## Most Common Patterns You Will See in This Codebase

### 1. `Option<T>` — "this might be missing"

```rust
collection: Option<String>,     // like `str | None` or `string | undefined`
```

- `Some("foo")` or `None`
- `if let Some(name) = &cfg.collections { ... }`
- `.unwrap_or_default()`, `.unwrap_or(0)`, `.unwrap_or_else(|| ...)`

**Python equivalent**: `Optional[str]`, `x if x is not None else default`

### 2. `Result<T, E>` + the `?` operator

```rust
fn load_config() -> Result<QmdConfig> {
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    ...
}
```

The `?` means: "If this failed, return the error from the current function right now."

This replaces 90% of `try/except` blocks you would write in Python or `.catch()` chains in JS.

We use `anyhow::Result` because it is the most ergonomic choice for CLI applications — it can hold any error type and still lets us add nice `.context("...")` messages.

### 3. `&str` vs `String`

- `String` = owned heap data (you are responsible for it, like a Python `str` that you can mutate).
- `&str` = a borrowed view / slice (cheap, read-only, like passing a `const char*` or a Python `memoryview`).

Rule of thumb in this project:
- Function parameters that don't need to own the data → use `&str`
- When you need to store or return a string you created → use `String`

### 4. Exhaustive `match` and `if let`

```rust
match cli.command {
    Some(Commands::Status { json }) => { ... }
    Some(Commands::Init { force }) => { ... }
    // ... every other variant must be listed or the compiler will refuse to compile
}
```

The compiler will not let you forget a case. This eliminates an entire class of bugs that are common when using string command names + giant `if/elif` ladders.

### 5. Serde + `#[derive(Deserialize)]`

```rust
#[derive(Debug, Deserialize, Default)]
struct QmdConfig { ... }
```

This is Rust's equivalent of:
- Python: Pydantic `BaseModel` + `model_validate`
- TypeScript: Zod schema + `.parse()`

`serde_yaml::from_str` turns the YAML text into a real Rust struct with zero manual parsing code.

### 6. Closures passed to rusqlite

```rust
conn.query_row("SELECT ...", [], |row| { row.get(0) })
```

The `|row| { ... }` is a closure (anonymous function), very similar to an arrow function in JS/TS or a `lambda` in Python.

It receives a `Row` and must return the column you want. This design makes it almost impossible to use the row after the statement has been dropped (a common source of bugs in other SQLite bindings).

## How to Explore the Code as a Newcomer

1. Start at `src/main.rs` — it is heavily commented for exactly this audience.
2. Read the top `//!` module docs first.
3. Look at the `Cli` and `Commands` enum definitions — this is where the entire CLI surface is declared.
4. Jump to `cmd_status` and the three helper functions at the bottom (`load_config`, `db_counts`, `last_updated_hint`). These are the only pieces that currently do "real work" (YAML + SQLite).
5. When you see a new pattern you don't recognize, search for it in this wiki page or ask for an explanation — we treat teaching comments as first-class.

## Recommended Learning Resources (while working on qmd-rust)

- "The Rust Book" (free) — especially chapters on Ownership, Structs, Enums, Error Handling, and Traits.
- "Rust for Rustaceans" (book) — once you are comfortable with the basics.
- `cargo expand` (install with `cargo install cargo-expand`) — lets you see what the `#[derive(Parser)]` macro actually generates. Extremely educational.
- Run `cargo clippy -- -D warnings` often — the lints are fantastic teachers.

## How This Page Will Grow

As we port more of the original TypeScript (`store.ts`, `db.ts`, `llm.ts`, tree-sitter chunking, MCP server, etc.), new sections will be added here covering:
- Async (`tokio`) vs Python `asyncio`
- FFI safety (sqlite-vec, llama.cpp)
- How we will eventually split the binary into a library + multiple binaries
- Testing patterns (`#[test]`, integration tests against the real Node `qmd` output)

---

**Contributing to this page**: If you (or an agent) discover a new "I wish someone had told me this earlier" moment while reading the Rust code, please add a short, concrete section here with a code example from the project. The comments in `src/main.rs` are the raw material — this page is the curated, higher-level version.
