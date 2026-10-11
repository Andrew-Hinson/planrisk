//! planrisk: reads a Terraform plan and flags risky changes.
//!
//! The code lives in this library crate, and `main.rs` is a thin wrapper
//! around it. This is a common layout for Rust CLIs: tests in `tests/` can
//! only import from a library, not from a binary.

pub mod action;
pub mod finding;
pub mod input;
pub mod json;
pub mod plan;
pub mod redact;
pub mod report;
pub mod rules;
pub mod summary;
