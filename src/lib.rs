//! Core library for the `alfred-gh` Alfred workflow.
//!
//! The binary is a thin dispatcher over these modules. Everything that can be
//! made pure (parsing, ranking, caching decisions, item building) lives here
//! and is unit tested; the only impure boundary is [`runner::CommandRunner`],
//! which is mocked in tests, plus the cache directory.

pub mod action;
pub mod alfred;
pub mod app;
pub mod cache;
pub mod gh;
pub mod local;
pub mod owners;
pub mod query;
pub mod ranking;
pub mod repos;
pub mod runner;
pub mod time;
