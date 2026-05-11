//! imauth-core — clean-architecture layering for the auth orchestration crate.
//!
//! Modules are arranged in dependency order:
//!  - `domain`: pure types and logic (no I/O, no async, no Arc).
//!  - `ports`: trait definitions the application layer depends on.
//!  - `application`: use cases that coordinate workflows through ports, plus the
//!    `AppContainer` composition root.
//!  - `adapters`: concrete implementations of ports (sqlite, chromiumoxide,
//!    aes_gcm, nats, fs, in-memory).
//!
//! The boundary direction is `adapters → ports → application → domain` and is
//! enforced by review (see CLAUDE.md). The `config` and `error` modules are
//! shared at the crate root.

pub mod adapters;
pub mod application;
pub mod config;
pub mod domain;
pub mod error;
pub mod ports;

#[cfg(test)]
mod proto_lint_tests;

pub use application::AppContainer;
pub use config::Config;
pub use error::{ImauthError, Result};
