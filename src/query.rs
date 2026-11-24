//! Parsing of what the user types after each keyword.

use crate::owners;
use crate::prs::PrState;

/// `ghp` / `gha`: `<repo> <rest>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoQuery {
    /// The repository as typed (`owner/name` or a bare name), possibly empty.
    pub repo: String,
    /// Everything after the repository.
    pub rest: String,
}

/// Split a `ghp` / `gha` query into the repository and the rest.
pub fn parse_repo(query: &str) -> RepoQuery {
    let query = query.trim_start();

    match query.split_once(char::is_whitespace) {
        Some((repo, rest)) => RepoQuery {
            repo: repo.to_owned(),
            rest: rest.trim().to_owned(),
        },
        None => RepoQuery {
            repo: query.to_owned(),
            rest: String::new(),
        },
    }
}

/// The rest of a `ghp` query: a `state:open|merged|all` filter (default
/// open) and the text to filter by. An unknown state is kept as text.
pub fn parse_pr_filter(rest: &str) -> (PrState, String) {
    let mut state = PrState::Open;
    let mut words = Vec::new();

    for word in rest.split_whitespace() {
        let parsed = word.strip_prefix("state:").and_then(PrState::parse);

        match parsed {
            Some(parsed) => state = parsed,
            None => words.push(word),
        }
    }

    (state, words.join(" "))
}

/// `gho [<owner>:]<text>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReposQuery {
    /// Restrict to this owner (a login from `GH_OWNERS`).
    pub owner: Option<String>,
    /// What to rank repositories by.
    pub text: String,
}

/// Parse a `gho` query. A leading `x:` (first letter of an owner) or
/// `login:` restricts the list to that owner; an unknown or ambiguous prefix
/// is searched as text.
pub fn parse_repos(query: &str, owners: &[String]) -> ReposQuery {
    let query = query.trim();

    if let Some((prefix, rest)) = query.split_once(':') {
        if !prefix.contains(char::is_whitespace) {
            if let Some(owner) = owners::resolve_prefix(prefix, owners) {
                return ReposQuery {
                    owner: Some(owner.clone()),
                    text: rest.trim().to_owned(),
                };
            }
        }
    }

    ReposQuery {
        owner: None,
        text: query.to_owned(),
    }
}
