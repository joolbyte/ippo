//! Core library for ippo.
//!
//! The executable interfaces live around this library. Domain and persistence
//! behavior should remain usable without a live terminal.

pub mod app;
pub mod clock;
pub mod config;
pub mod diagnostics;
pub mod habit;
pub mod storage;
pub mod tui;
