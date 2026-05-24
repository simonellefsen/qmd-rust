//! Minimal QmdError / QmdResult foundation (seed for future shared logic).
//!
//! This module provides the smallest viable typed error + atomic exit code
//! foundation. It serves as the starting point for reusable shared logic under
//! the utils/ area (per established crate layout conventions).
//!
//! For Rust newbies (from Python/Node/TS):
//! - `trait QmdError` + `Box<dyn QmdError>` is like a typed "interface" for
//!   errors that know their preferred exit code (no more magic 1/2 everywhere).
//! - `AtomicI32` + `Ordering::SeqCst` gives a thread-safe global "sticky" exit
//!   code for cases where we want to warn but keep going (then finalize at end).
//! - `?` and `Result` are still used; this just gives precise control when we
//!   need a non-1 exit status.
//!
//! QmdError trait / Qmd* types are the intentional seed for future shared
//! logic slices; only the atomic helpers are exercised in this minimal slice
//! per the one-site rule. No big anyhow replacement. See main.rs for usage.

use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicI32, Ordering};

/// Every error type used by qmd commands must implement this for exit code control.
/// `code()` returns the POSIX-style exit status (0=success, 1=error, 2=usage, etc.).
/// `usage()` defaults to false; override if this error should also trigger --help.
pub trait QmdError: Error + Send + Sync + 'static {
    fn code(&self) -> i32;
    fn usage(&self) -> bool {
        false
    }
}

/// Convenience alias (QmdResult<T> = Result<T, Box<dyn QmdError>>).
/// Mirrors UResult in uucore; lets command fns declare "I return qmd-flavored errors".
pub type QmdResult<T> = Result<T, Box<dyn QmdError>>;

/// Trivial error carrying a message and explicit exit code.
/// Use `QmdSimpleError::new(2, "bad usage")` etc. for the common case.
#[derive(Debug)]
pub struct QmdSimpleError {
    code: i32,
    message: String,
}

impl fmt::Display for QmdSimpleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for QmdSimpleError {}

impl QmdError for QmdSimpleError {
    fn code(&self) -> i32 {
        self.code
    }
}

impl QmdSimpleError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        QmdSimpleError {
            code,
            message: message.into(),
        }
    }
}

/// Global sticky exit code (AtomicI32 for Send+Sync safety across threads).
/// Use set/get for non-fatal "continue but exit N at the end" flows.
/// Default 0 (success) unless a command path calls set_exit_code.
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Record a non-zero exit code that will be used by the finalizer at process end.
/// Safe to call multiple times; last writer wins (or use max if preferred later).
pub fn set_exit_code(code: i32) {
    EXIT_CODE.store(code, Ordering::SeqCst);
}

/// Read the currently staged exit code (0 unless something called set).
pub fn get_exit_code() -> i32 {
    EXIT_CODE.load(Ordering::SeqCst)
}

/// Small finalize helper intended for use in main.rs (or a future #[qmd_main] macro).
/// After the command dispatch match, call this; if a sticky code was set, exit with it.
/// Example:
///     let code = qmd::utils::finalize_exit_code();
///     if code != 0 { std::process::exit(code); }
///     Ok(())
pub fn finalize_exit_code() -> i32 {
    get_exit_code()
}
