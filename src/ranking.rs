//! Filtering and ranking, done in Rust (`alfredfiltersresults = false`).

use std::cmp::Reverse;

use crate::local;
use crate::prs::Pr;
use crate::repos::Repo;

/// What a repository typed in `ghp` / `gha` resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// One repository, as `owner/name`.
    Exact(String),
    /// Several (or partial) matches to choose from, best first.
    Candidates(Vec<Repo>),
    /// Nothing matches.
    Unknown,
}

/// Resolve `spec` against the cached repositories. `owner/name` resolves to
/// itself (with GitHub's casing when cached), even outside the configured
/// owners; a bare name resolves when exactly one repository has that name;
/// anything else lists the matching repositories.
pub fn resolve_repo(repos: &[Repo], spec: &str) -> Resolution {
    let spec = spec.trim();

    if spec.is_empty() {
        return Resolution::Candidates(rank_repos(repos.to_vec(), ""));
    }

    let lower = spec.to_lowercase();

    if spec.contains('/') {
        if let Some(repo) = repos
            .iter()
            .find(|repo| repo.name_with_owner.to_lowercase() == lower)
        {
            return Resolution::Exact(repo.name_with_owner.clone());
        }

        if local::is_valid_name_with_owner(spec) {
            return Resolution::Exact(spec.to_owned());
        }

        return Resolution::Unknown;
    }

    let exact: Vec<&Repo> = repos
        .iter()
        .filter(|repo| repo.name().to_lowercase() == lower)
        .collect();

    if let [only] = exact.as_slice() {
        return Resolution::Exact(only.name_with_owner.clone());
    }

    let candidates = rank_repos(repos.to_vec(), spec);

    if candidates.is_empty() {
        return Resolution::Unknown;
    }

    Resolution::Candidates(candidates)
}

/// Keep the pull requests whose title, author, head branch or number
/// (`42` or `#42`) contains every word of `text`, in their original order.
pub fn filter_prs(prs: Vec<Pr>, text: &str) -> Vec<Pr> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect();

    prs.into_iter()
        .filter(|pr| {
            let haystack = format!(
                "{} {} {} #{}",
                pr.title.to_lowercase(),
                pr.author.as_deref().unwrap_or_default().to_lowercase(),
                pr.head.to_lowercase(),
                pr.number
            );

            words.iter().all(|word| {
                haystack.contains(word.as_str())
                    || word.trim_start_matches('#') == pr.number.to_string()
            })
        })
        .collect()
}

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
