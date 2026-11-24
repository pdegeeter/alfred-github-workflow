//! Pull requests, shared by `ghp` (one repository), `ghr` and `ghm` (search
//! across owners).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gh::{Gh, GhError};

/// The fields read for every pull request.
macro_rules! pr_fields {
    () => {
        "number title url state isDraft updatedAt headRefName baseRefName reviewDecision author { login } repository { nameWithOwner } commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }"
    };
}

/// A repository's pull requests for a fixed set of states. List variables
/// cannot travel as raw `-f` fields, so each state set is its own constant.
macro_rules! repo_prs_query {
    ($states:literal) => {
        concat!(
            "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { pullRequests(first: 50, ",
            $states,
            "orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { ",
            pr_fields!(),
            " } } } }"
        )
    };
}

/// Open pull requests of a repository, most recently updated first.
pub const OPEN_PRS_QUERY: &str = repo_prs_query!("states: [OPEN], ");

/// Merged pull requests of a repository.
pub const MERGED_PRS_QUERY: &str = repo_prs_query!("states: [MERGED], ");

/// Pull requests of a repository in any state.
pub const ALL_PRS_QUERY: &str = repo_prs_query!("");

/// Which pull requests `ghp` lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrState {
    Open,
    Merged,
    All,
}

impl PrState {
    /// From the value of a `state:` filter.
    pub fn parse(value: &str) -> Option<PrState> {
        match value.to_lowercase().as_str() {
            "open" => Some(PrState::Open),
            "merged" => Some(PrState::Merged),
            "all" => Some(PrState::All),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PrState::Open => "open",
            PrState::Merged => "merged",
            PrState::All => "all",
        }
    }

    fn query(self) -> &'static str {
        match self {
            PrState::Open => OPEN_PRS_QUERY,
            PrState::Merged => MERGED_PRS_QUERY,
            PrState::All => ALL_PRS_QUERY,
        }
    }
}

/// Combined status of the head commit's checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Checks {
    Success,
    Failure,
    Pending,
}

impl Checks {
    fn parse(value: &str) -> Option<Checks> {
        match value {
            "SUCCESS" => Some(Checks::Success),
            "FAILURE" | "ERROR" => Some(Checks::Failure),
            "PENDING" | "EXPECTED" => Some(Checks::Pending),
            _ => None,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Checks::Success => "✅",
            Checks::Failure => "❌",
            Checks::Pending => "🟡",
        }
    }
}

/// A pull request as listed and cached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// `OPEN`, `MERGED` or `CLOSED`.
    pub state: String,
    pub is_draft: bool,
    pub updated_at: String,
    pub head: String,
    pub base: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, if any.
    pub review_decision: Option<String>,
    /// `None` for deleted accounts.
    pub author: Option<String>,
    pub repo: String,
    pub checks: Option<Checks>,
}

impl Pr {
    /// The "Files changed" page.
    pub fn files_url(&self) -> String {
        format!("{}/files", self.url)
    }

    /// A readable review state.
    pub fn review_label(&self) -> Option<&'static str> {
        match self.review_decision.as_deref()? {
            "APPROVED" => Some("approved"),
            "CHANGES_REQUESTED" => Some("changes requested"),
            "REVIEW_REQUIRED" => Some("review required"),
            _ => None,
        }
    }

    /// Parse one pull request node.
    pub fn from_node(node: &Value) -> Option<Pr> {
        let text = |value: &Value| value.as_str().map(str::to_owned);

        Some(Pr {
            number: node["number"].as_u64()?,
            title: text(&node["title"])?,
            url: text(&node["url"])?,
            state: text(&node["state"]).unwrap_or_else(|| "OPEN".to_owned()),
            is_draft: node["isDraft"].as_bool().unwrap_or(false),
            updated_at: text(&node["updatedAt"]).unwrap_or_default(),
            head: text(&node["headRefName"]).unwrap_or_default(),
            base: text(&node["baseRefName"]).unwrap_or_default(),
            review_decision: text(&node["reviewDecision"]),
            author: text(&node["author"]["login"]),
            repo: text(&node["repository"]["nameWithOwner"])?,
            checks: node["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["state"]
                .as_str()
                .and_then(Checks::parse),
        })
    }
}

fn parse_nodes(nodes: &Value) -> Vec<Pr> {
    nodes
        .as_array()
        .map(|nodes| nodes.iter().filter_map(Pr::from_node).collect())
        .unwrap_or_default()
}

/// Parse the `data` of a repository pull request query.
pub fn parse_repo_prs(data: &Value) -> Option<Vec<Pr>> {
    let repository = data.get("repository").filter(|repo| !repo.is_null())?;

    Some(parse_nodes(&repository["pullRequests"]["nodes"]))
}

/// Fetch the pull requests of `repo` (`owner/name`) in `state`.
pub fn fetch_repo_prs(gh: &Gh, repo: &str, state: PrState) -> Result<Vec<Pr>, GhError> {
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(|| GhError::Other(format!("invalid repository: {repo}")))?;

    let data = gh.graphql(state.query(), &[("owner", owner), ("name", name)])?;

    parse_repo_prs(&data).ok_or_else(|| GhError::Other(format!("{repo}: repository not found")))
}
