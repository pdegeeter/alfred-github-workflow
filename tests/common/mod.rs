//! Shared test helpers: a deterministic, offline [`CommandRunner`] double.
//!
//! Included by multiple test crates; each uses a different subset, so unused
//! helpers are expected per-crate.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use alfred_gh::runner::{CommandError, CommandRunner};
use anyhow::{bail, Result};

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
