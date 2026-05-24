//! Shared utilities (errors, FS helpers, output formatters, etc.).
//! See error.rs for the QmdError seed (future home of qmd_core pieces).

pub mod error;

pub use error::{
    finalize_exit_code, get_exit_code, set_exit_code, QmdError, QmdResult, QmdSimpleError,
};
