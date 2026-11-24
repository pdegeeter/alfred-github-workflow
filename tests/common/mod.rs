//! Shared test helpers: a deterministic, offline [`CommandRunner`] double.
//!
//! Included by multiple test crates; each uses a different subset, so unused
//! helpers are expected per-crate.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alfred_gh::app::{App, Settings};
use alfred_gh::cache::Cache;
use alfred_gh::runner::{CommandError, CommandRunner};
use anyhow::{bail, Result};
use serde_json::Value;

/// The `gh` path the tests pretend is installed.
pub const GH: &str = "/opt/homebrew/bin/gh";

/// The binary path the tests pretend `alfred-gh` runs from.
pub const EXE: &str = "/Alfred/workflows/gh/alfred-gh";

/// The clock of every app test: 2026-11-25T00:00:00Z.
pub const NOW: u64 = 1_795_564_800;

pub fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

/// An app on `runner`, caching in `dir`, at `now`, for `owners`
/// (`GH_OWNERS`).
pub fn app<'a>(runner: &'a FakeRunner, dir: &Path, now: u64, owners: &str) -> App<'a> {
    App {
        runner,
        gh_path: Some(GH),
        cache: Cache::new(dir, at(now)),
        exe: EXE.to_owned(),
        home: "/Users/octo".into(),
        settings: Settings::from_vars(|name| match name {
            "GH_OWNERS" => Some(owners.to_owned()),
            "JIRA_URL" => Some("https://jira.example.com/browse/".to_owned()),
            _ => None,
        }),
    }
}

/// Items as the JSON Alfred receives.
pub fn items_json(items: &[powerpack::Item]) -> Vec<Value> {
    items
        .iter()
        .map(|item| serde_json::to_value(item).unwrap())
        .collect()
}

/// The titles of `items`.
pub fn titles(items: &[powerpack::Item]) -> Vec<String> {
    items_json(items)
        .iter()
        .map(|item| item["title"].as_str().unwrap().to_owned())
        .collect()
}

/// A single recorded invocation of [`FakeRunner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
enum Response {
    Success(String),
    Failure { stdout: String, stderr: String },
}

/// A [`CommandRunner`] that returns canned output keyed by program name and
/// records every call, so tests stay offline and can assert on invocations.
///
/// Responses queued for a program are consumed in order; the last one keeps
/// answering once the queue is down to it. Detached spawns always succeed and
/// are recorded separately in `spawns`.
pub struct FakeRunner {
    responses: RefCell<HashMap<String, VecDeque<Response>>>,
    pub calls: RefCell<Vec<Call>>,
    pub spawns: RefCell<Vec<Call>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self {
            responses: RefCell::new(HashMap::new()),
            calls: RefCell::new(Vec::new()),
            spawns: RefCell::new(Vec::new()),
        }
    }

    /// Queue a successful run of `program` printing `stdout`.
    pub fn with_success(self, program: &str, stdout: &str) -> Self {
        self.push(program, Response::Success(stdout.to_owned()));

        self
    }

    /// Queue a failed run of `program` printing `message` on stderr.
    pub fn with_failure(self, program: &str, message: &str) -> Self {
        self.with_failure_output(program, "", message)
    }

    /// Queue a failed run of `program` printing `stdout` and `stderr`.
    pub fn with_failure_output(self, program: &str, stdout: &str, stderr: &str) -> Self {
        self.push(
            program,
            Response::Failure {
                stdout: stdout.to_owned(),
                stderr: stderr.to_owned(),
            },
        );

        self
    }

    /// Every recorded (waited-for) call.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    /// Every recorded detached spawn.
    pub fn spawns(&self) -> Vec<Call> {
        self.spawns.borrow().clone()
    }

    /// The programs invoked so far, in order.
    pub fn programs_called(&self) -> Vec<String> {
        self.calls().into_iter().map(|call| call.program).collect()
    }

    fn push(&self, program: &str, response: Response) {
        self.responses
            .borrow_mut()
            .entry(program.to_owned())
            .or_default()
            .push_back(response);
    }

    fn record(program: &str, args: &[&str]) -> Call {
        Call {
            program: program.to_owned(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
        }
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<String> {
        self.calls
            .borrow_mut()
            .push(FakeRunner::record(program, args));

        let mut responses = self.responses.borrow_mut();
        let Some(queue) = responses.get_mut(program) else {
            bail!("unexpected program: `{program}`");
        };

        let response = if queue.len() > 1 {
            queue.pop_front().expect("queue is not empty")
        } else {
            queue.front().cloned().expect("queue is not empty")
        };

        match response {
            Response::Success(stdout) => Ok(stdout),
            Response::Failure { stdout, stderr } => Err(CommandError {
                program: program.to_owned(),
                status: Some(1),
                stdout,
                stderr,
            }
            .into()),
        }
    }

    fn spawn_detached(&self, program: &str, args: &[&str]) -> Result<()> {
        self.spawns
            .borrow_mut()
            .push(FakeRunner::record(program, args));

        Ok(())
    }
}
