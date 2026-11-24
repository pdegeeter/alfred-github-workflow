//! Filtering and ranking, done in Rust (`alfredfiltersresults = false`).

use std::cmp::Reverse;

use crate::repos::Repo;

/// How well a repository matches a query, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    ExactName,
    NamePrefix,
    NameSubstring,
    Topic,
    Description,
}

fn tier(repo: &Repo, query: &str) -> Option<Tier> {
    if query.is_empty() {
        return Some(Tier::ExactName);
    }

    let name = repo.name().to_lowercase();

    if name == query {
        return Some(Tier::ExactName);
    }

    if name.starts_with(query) {
        return Some(Tier::NamePrefix);
    }

    if name.contains(query) {
        return Some(Tier::NameSubstring);
    }

    if repo
        .topics
        .iter()
        .any(|topic| topic.to_lowercase().contains(query))
    {
        return Some(Tier::Topic);
    }

    let in_description = repo
        .description
        .as_deref()
        .is_some_and(|description| description.to_lowercase().contains(query));

    if in_description {
        return Some(Tier::Description);
    }

    None
}

/// Keep the repositories matching `query` and order them: archived last, then
/// exact name > name prefix > name substring > topic > description, ties
/// broken by the most recent push. An empty query keeps everything.
pub fn rank_repos(repos: Vec<Repo>, query: &str) -> Vec<Repo> {
    let query = query.trim().to_lowercase();

    let mut ranked: Vec<(Tier, Repo)> = repos
        .into_iter()
        .filter_map(|repo| tier(&repo, &query).map(|tier| (tier, repo)))
        .collect();

    ranked.sort_by(|(tier_a, a), (tier_b, b)| {
        (a.is_archived, tier_a, Reverse(&a.pushed_at)).cmp(&(
            b.is_archived,
            tier_b,
            Reverse(&b.pushed_at),
        ))
    });

    ranked.into_iter().map(|(_, repo)| repo).collect()
}
