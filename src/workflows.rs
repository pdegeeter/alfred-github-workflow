//! GitHub Actions workflows and runs, through the REST API (`gh api`).
//!
//! The last run of each workflow on the default branch comes from a single
//! `actions/runs?branch=<default>&per_page=100` call grouped by workflow, not
//! one call per workflow. A workflow with no run among those 100 shows
//! "no recent run".

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gh::{Gh, GhError};
use crate::local;
use crate::time;

/// How many runs `gha <repo> runs:<id>` lists.
pub const RUNS_PER_WORKFLOW: usize = 10;

/// A workflow of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    /// `.github/workflows/ci.yml`.
    pub path: String,
    /// `active`, `disabled_manually`, `disabled_inactivity`…
    pub state: String,
}

impl Workflow {
    pub fn is_disabled(&self) -> bool {
        self.state.starts_with("disabled")
    }

    /// The workflow's runs page on GitHub.
    pub fn runs_url(&self, repo: &str) -> String {
        let file = self.path.rsplit('/').next().unwrap_or(&self.path);

        format!("https://github.com/{repo}/actions/workflows/{file}")
    }
}

/// The workflows of a repository and its default branch: one cache entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowList {
    pub default_branch: String,
    pub workflows: Vec<Workflow>,
}

/// One workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: u64,
    pub workflow_id: u64,
    pub run_number: u64,
    /// The commit message or pull request title that triggered it.
    pub title: String,
    pub branch: String,
    pub event: String,
    pub actor: Option<String>,
    /// `queued`, `in_progress`, `completed`…
    pub status: String,
    /// `success`, `failure`, `cancelled`… once completed.
    pub conclusion: Option<String>,
    pub url: String,
    pub started_at: String,
    pub updated_at: String,
}

/// What a run's icon says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Success,
    Failure,
    InProgress,
    /// Cancelled, skipped, neutral…
    Other,
}

impl RunStatus {
    pub fn icon(self) -> &'static str {
        match self {
            RunStatus::Success => "✅",
            RunStatus::Failure => "❌",
            RunStatus::InProgress => "🟡",
            RunStatus::Other => "⚪",
        }
    }
}

impl Run {
    pub fn status(&self) -> RunStatus {
        if self.status != "completed" {
            return RunStatus::InProgress;
        }

        match self.conclusion.as_deref() {
            Some("success") => RunStatus::Success,
            Some("failure" | "timed_out" | "startup_failure" | "action_required") => {
                RunStatus::Failure
            }
            _ => RunStatus::Other,
        }
    }

    /// How long it ran (or has been running, at `now`), in seconds.
    pub fn duration(&self, now: i64) -> Option<i64> {
        let started = time::parse_iso8601(&self.started_at)?;

        let end = if self.status == "completed" {
            time::parse_iso8601(&self.updated_at)?
        } else {
            now
        };

        Some(end - started)
    }

    fn from_json(run: &Value) -> Option<Run> {
        let text = |value: &Value| value.as_str().map(str::to_owned);

        Some(Run {
            id: run["id"].as_u64()?,
            workflow_id: run["workflow_id"].as_u64()?,
            run_number: run["run_number"].as_u64().unwrap_or_default(),
            title: text(&run["display_title"])
                .or_else(|| text(&run["name"]))
                .unwrap_or_default(),
            branch: text(&run["head_branch"]).unwrap_or_default(),
            event: text(&run["event"]).unwrap_or_default(),
            actor: text(&run["actor"]["login"]),
            status: text(&run["status"]).unwrap_or_default(),
            conclusion: text(&run["conclusion"]),
            url: text(&run["html_url"])?,
            started_at: text(&run["run_started_at"])
                .or_else(|| text(&run["created_at"]))
                .unwrap_or_default(),
            updated_at: text(&run["updated_at"]).unwrap_or_default(),
        })
    }
}

/// Parse `GET repos/{repo}/actions/workflows`.
pub fn parse_workflows(body: &Value) -> Vec<Workflow> {
    body["workflows"]
        .as_array()
        .map(|workflows| {
            workflows
                .iter()
                .filter_map(|workflow| {
                    Some(Workflow {
                        id: workflow["id"].as_u64()?,
                        name: workflow["name"].as_str()?.to_owned(),
                        path: workflow["path"].as_str().unwrap_or_default().to_owned(),
                        state: workflow["state"].as_str().unwrap_or_default().to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a `workflow_runs` list (newest first, as GitHub returns it).
pub fn parse_runs(body: &Value) -> Vec<Run> {
    body["workflow_runs"]
        .as_array()
        .map(|runs| runs.iter().filter_map(Run::from_json).collect())
        .unwrap_or_default()
}

/// The newest run of `workflow_id` among `runs` (newest first).
pub fn last_run(runs: &[Run], workflow_id: u64) -> Option<&Run> {
    runs.iter().find(|run| run.workflow_id == workflow_id)
}

/// Percent-encode a query string value (branch names may hold `/`, `&`, `#`…).
pub fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn checked(repo: &str) -> Result<&str, GhError> {
    if !local::is_valid_name_with_owner(repo) {
        return Err(GhError::Other(format!("invalid repository: {repo}")));
    }

    Ok(repo)
}

/// Fetch the workflows of `repo`. `default_branch` comes from the repository
/// cache when known; otherwise it is read from `GET repos/{repo}`.
pub fn fetch_workflows(
    gh: &Gh,
    repo: &str,
    default_branch: Option<&str>,
) -> Result<WorkflowList, GhError> {
    let repo = checked(repo)?;

    let default_branch = match default_branch {
        Some(branch) => branch.to_owned(),
        None => gh.rest(&format!("repos/{repo}"))?["default_branch"]
            .as_str()
            .unwrap_or("main")
            .to_owned(),
    };

    let body = gh.rest(&format!("repos/{repo}/actions/workflows?per_page=100"))?;

    Ok(WorkflowList {
        default_branch,
        workflows: parse_workflows(&body),
    })
}

/// Fetch the latest 100 runs of `repo` on `branch`.
pub fn fetch_branch_runs(gh: &Gh, repo: &str, branch: &str) -> Result<Vec<Run>, GhError> {
    let repo = checked(repo)?;
    let body = gh.rest(&format!(
        "repos/{repo}/actions/runs?branch={}&per_page=100",
        encode(branch)
    ))?;

    Ok(parse_runs(&body))
}

/// Fetch the latest runs of one workflow, on any branch.
pub fn fetch_workflow_runs(gh: &Gh, repo: &str, workflow_id: u64) -> Result<Vec<Run>, GhError> {
    let repo = checked(repo)?;
    let body = gh.rest(&format!(
        "repos/{repo}/actions/workflows/{workflow_id}/runs?per_page={RUNS_PER_WORKFLOW}"
    ))?;

    Ok(parse_runs(&body))
}
