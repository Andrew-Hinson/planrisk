//! planrisk: reads a Terraform plan and flags risky changes.
//!
//! The code lives in this library crate, and `main.rs` is a thin wrapper
//! around it. This is a common layout for Rust CLIs: tests in `tests/` can
//! only import from a library, not from a binary.

pub mod action;
pub mod plan;
