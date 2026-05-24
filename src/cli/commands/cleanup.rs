//! `qmd cleanup` command.
//!
//! Extracted from mod.rs in the review fix round (to resolve the high-impact
//! "monolith in mod.rs" architectural/maintainability finding #1 while
//! obeying the task constraints on the original implementation pass).
//!
//! For Rust newbies: direct SQL + VACUUM via the shared open_connection helper,
//! exactly as other maintenance paths. Graceful handling for tables that may
//! not exist in a pure-Rust-created index (llm_cache is primarily populated by
//! the Node reference today).

use crate::db::open_connection;
use anyhow::Result;

pub fn cmd_cleanup() -> Result<()> {
    let conn = open_connection(false)?;
    let cache: u32 = conn.execute("DELETE FROM llm_cache", []).unwrap_or(0) as u32;

    // Orphaned vectors (no active document) — plain table, no virtual table touch.
    let orphaned_vec: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM content_vectors cv WHERE NOT EXISTS (SELECT 1 FROM documents d WHERE d.hash = cv.hash AND d.active = 1)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if orphaned_vec > 0 {
        let _ = conn.execute(
            "DELETE FROM content_vectors WHERE hash NOT IN (SELECT hash FROM documents WHERE active = 1)",
            [],
        );
    }

    let inactive: u32 = conn
        .execute("DELETE FROM documents WHERE active = 0", [])
        .unwrap_or(0) as u32;

    let orphaned_content: u32 = conn
        .execute(
            "DELETE FROM content WHERE hash NOT IN (SELECT DISTINCT hash FROM documents)",
            [],
        )
        .unwrap_or(0) as u32;

    let _ = conn.execute("VACUUM", []);

    // Polish for review nit #11: report cache count unconditionally (matches TS
    // style) but keep the "No orphaned..." conditional messaging for vecs.
    println!("✓ Cleared {} cached API responses", cache);
    if orphaned_vec > 0 {
        println!("✓ Removed {} orphaned embedding chunks", orphaned_vec);
    } else {
        println!("No orphaned embeddings to remove");
    }
    if inactive > 0 {
        println!("✓ Removed {} inactive document records", inactive);
    }
    if orphaned_content > 0 {
        println!("✓ Removed {} orphaned content rows", orphaned_content);
    }
    println!("✓ Database vacuumed");
    Ok(())
}
