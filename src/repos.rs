//! Repositories of an owner, fetched with `repositoryOwner(login:)`, which
//! works for organizations and user accounts alike (`organization(login:)`
//! fails for users). `ownerAffiliations: [OWNER]` keeps a user's list to the
//! repositories they own: without it GitHub adds every repository they
//! collaborate on, including other organizations'.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gh::{Gh, GhError};
use crate::owners::OwnerKind;

/// The fields read for every repository.
macro_rules! repo_fields {
    () => {
        "nameWithOwner description url homepageUrl pushedAt isArchived isPrivate isFork primaryLanguage { name } repositoryTopics(first: 10) { nodes { topic { name } } } defaultBranchRef { name }"
    };
}

/// One page of an owner's repositories, most recently pushed first.
pub const REPOS_QUERY: &str = concat!(
    "query($login: String!, $after: String) { repositoryOwner(login: $login) { __typename repositories(first: 100, after: $after, ownerAffiliations: [OWNER], orderBy: {field: PUSHED_AT, direction: DESC}) { pageInfo { hasNextPage endCursor } nodes { ",
    repo_fields!(),
    " } } } }"
);

/// Public repository search (`gh` keyword), best match first.
pub const SEARCH_REPOS_QUERY: &str = concat!(
    "query($q: String!) { search(type: REPOSITORY, first: 20, query: $q) { nodes { ... on Repository { ",
    repo_fields!(),
    " } } } }"
);

/// Safety net against a cursor that never ends (100 pages = 10,000 repos).
const MAX_PAGES: usize = 100;

/// A repository as listed and cached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repo {
    pub name_with_owner: String,
    pub description: Option<String>,
    pub url: String,
    pub homepage_url: Option<String>,
    pub pushed_at: Option<String>,
    pub is_archived: bool,
    pub is_private: bool,
    pub is_fork: bool,
    pub language: Option<String>,
    pub topics: Vec<String>,
    pub default_branch: Option<String>,
}

impl Repo {
    /// The owner login, as GitHub spells it.
    pub fn owner(&self) -> &str {
        self.name_with_owner
            .split_once('/')
            .map_or(self.name_with_owner.as_str(), |(owner, _)| owner)
    }

    /// The repository name without its owner.
    pub fn name(&self) -> &str {
        self.name_with_owner
            .split_once('/')
            .map_or(self.name_with_owner.as_str(), |(_, name)| name)
    }

    fn from_node(node: &Value) -> Option<Repo> {
        let text = |value: &Value| {
            value
                .as_str()
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };

        Some(Repo {
            name_with_owner: node["nameWithOwner"].as_str()?.to_owned(),
            description: text(&node["description"]),
            url: node["url"].as_str()?.to_owned(),
            homepage_url: text(&node["homepageUrl"]),
            pushed_at: text(&node["pushedAt"]),
            is_archived: node["isArchived"].as_bool().unwrap_or(false),
            is_private: node["isPrivate"].as_bool().unwrap_or(false),
            is_fork: node["isFork"].as_bool().unwrap_or(false),
            language: text(&node["primaryLanguage"]["name"]),
            topics: node["repositoryTopics"]["nodes"]
                .as_array()
                .map(|topics| {
                    topics
                        .iter()
                        .filter_map(|topic| topic["topic"]["name"].as_str())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            default_branch: text(&node["defaultBranchRef"]["name"]),
        })
    }
}

/// An owner, its kind and all its repositories: one cache entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerRepos {
    pub login: String,
    pub kind: OwnerKind,
    pub repos: Vec<Repo>,
}

/// One parsed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub kind: OwnerKind,
    pub repos: Vec<Repo>,
    /// The cursor of the next page, if there is one.
    pub next: Option<String>,
}

/// Parse the `data` of [`REPOS_QUERY`]. `None` when the owner does not exist.
pub fn parse_page(data: &Value) -> Option<Page> {
    let owner = &data["repositoryOwner"];
    let kind = OwnerKind::from_typename(owner["__typename"].as_str()?)?;
    let repositories = &owner["repositories"];

    let repos = repositories["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().filter_map(Repo::from_node).collect())
        .unwrap_or_default();

    let next = if repositories["pageInfo"]["hasNextPage"] == true {
        repositories["pageInfo"]["endCursor"]
            .as_str()
            .map(str::to_owned)
    } else {
        None
    };

    Some(Page { kind, repos, next })
}

/// Parse the `data` of [`SEARCH_REPOS_QUERY`].
pub fn parse_search(data: &Value) -> Vec<Repo> {
    data["search"]["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().filter_map(Repo::from_node).collect())
        .unwrap_or_default()
}

/// Search public GitHub repositories (live, never cached).
pub fn search(gh: &Gh, query: &str) -> Result<Vec<Repo>, GhError> {
    let data = gh.graphql(SEARCH_REPOS_QUERY, &[("q", query)])?;

    Ok(parse_search(&data))
}

/// Fetch every repository of `login`, following the pagination cursor.
pub fn fetch_owner(gh: &Gh, login: &str) -> Result<OwnerRepos, GhError> {
    let mut repos = Vec::new();
    let mut after: Option<String> = None;
    let mut kind = None;

    for _ in 0..MAX_PAGES {
        let mut vars = vec![("login", login)];

        if let Some(cursor) = after.as_deref() {
            vars.push(("after", cursor));
        }

        let data = gh.graphql(REPOS_QUERY, &vars)?;
        let page = parse_page(&data)
            .ok_or_else(|| GhError::Other(format!("{login}: no such organization or user")))?;

        kind = Some(page.kind);
        repos.extend(page.repos);

        match page.next {
            Some(cursor) => after = Some(cursor),
            None => break,
        }
    }

    Ok(OwnerRepos {
        login: login.to_owned(),
        kind: kind.unwrap_or(OwnerKind::User),
        repos,
    })
}
