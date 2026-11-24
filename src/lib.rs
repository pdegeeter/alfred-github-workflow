//! Core library for the `alfred-gh` Alfred workflow.
//!
//! The binary is a thin dispatcher over these modules. Everything that can be
//! made pure (parsing, ranking, caching decisions, item building) lives here
//! and is unit tested; the only impure boundary is [`runner::CommandRunner`],
//! which is mocked in tests.

pub mod action;
pub mod alfred;
pub mod runner;
