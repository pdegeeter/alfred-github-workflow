//! Thin abstraction over the impure boundary: spawning external processes.
//!
//! Everything that talks to GitHub (`gh`), the clipboard and Alfred
//! (`osascript`) or macOS `open` goes through [`CommandRunner`]. Production
//! code uses [`SystemRunner`]; tests inject a fake so the rest of the crate
//! stays deterministic and offline.

use std::fmt;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Abstraction over running an external command.
pub trait CommandRunner {
    /// Run `program` with `args` and wait for it, returning the captured
    /// stdout on success. A non-zero exit returns a [`CommandError`] (reachable
    /// with `downcast_ref`) that keeps both stdout and stderr: `gh` prints
    /// GraphQL errors as JSON on stdout while exiting with a failure.
    fn run(&self, program: &str, args: &[&str]) -> Result<String>;

    /// Start `program` with `args` in the background and return at once,
    /// without waiting for it or capturing its output.
    fn spawn_detached(&self, program: &str, args: &[&str]) -> Result<()>;
}

/// A command that ran but exited with a failure status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    pub program: String,
    /// The exit code, `None` when the process was killed by a signal.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = self
            .status
            .map_or_else(|| "a signal".to_owned(), |code| code.to_string());

        write!(
            f,
            "`{}` exited with {status}: {}",
            self.program,
            self.stderr.trim()
        )
    }
}

impl std::error::Error for CommandError {}

/// Real implementation backed by [`std::process::Command`].
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("failed to spawn `{program}`"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

        if !output.status.success() {
            return Err(CommandError {
                program: program.to_owned(),
                status: output.status.code(),
                stdout,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }
            .into());
        }

        Ok(stdout)
    }

    fn spawn_detached(&self, program: &str, args: &[&str]) -> Result<()> {
        // All three streams are detached: Alfred waits for the script filter's
        // stdout to close, so an inherited stdout would block the dropdown.
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn `{program}`"))?;

        Ok(())
    }
}
