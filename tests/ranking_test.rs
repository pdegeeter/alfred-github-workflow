//! Tests for repository filtering and ranking.

use alfred_gh::ranking;
use alfred_gh::repos::Repo;

fn repo(name_with_owner: &str, pushed_at: &str) -> Repo {
    Repo {
        name_with_owner: name_with_owner.to_owned(),
        description: None,
        url: format!("https://github.com/{name_with_owner}"),
        homepage_url: None,
        pushed_at: Some(pushed_at.to_owned()),
        is_archived: false,
        is_private: false,
        is_fork: false,
        language: None,
        topics: Vec::new(),
        default_branch: Some("main".to_owned()),
    }
}

fn names(repos: &[Repo]) -> Vec<&str> {
    repos
        .iter()
        .map(|repo| repo.name_with_owner.as_str())
        .collect()
}

#[test]
fn exact_beats_prefix_beats_substring_beats_topic_beats_description() {
    let mut by_topic = repo("acme-org/billing", "2026-01-09T00:00:00Z");
    by_topic.topics = vec!["api-gateway".to_owned()];
    let mut by_description = repo("acme-org/web", "2026-01-10T00:00:00Z");
    by_description.description = Some("Talks to the API".to_owned());

    let ranked = ranking::rank_repos(
        vec![
            by_description,
            by_topic,
            repo("acme-org/legacy-api", "2026-01-08T00:00:00Z"),
            repo("acme-org/api-client", "2026-01-07T00:00:00Z"),
            repo("octo-user/api", "2026-01-01T00:00:00Z"),
            repo("acme-org/unrelated", "2026-01-11T00:00:00Z"),
        ],
        "API",
    );

    assert_eq!(
        names(&ranked),
        [
            "octo-user/api",
            "acme-org/api-client",
            "acme-org/legacy-api",
            "acme-org/billing",
            "acme-org/web"
        ]
    );
}

#[test]
fn ties_are_broken_by_the_most_recent_push() {
    let ranked = ranking::rank_repos(
        vec![
            repo("acme-org/api-old", "2025-01-01T00:00:00Z"),
            repo("acme-org/api-new", "2026-01-01T00:00:00Z"),
        ],
        "api",
    );

    assert_eq!(names(&ranked), ["acme-org/api-new", "acme-org/api-old"]);
}

#[test]
fn archived_repositories_come_last_even_when_they_match_better() {
    let mut archived = repo("acme-org/api", "2026-02-01T00:00:00Z");
    archived.is_archived = true;

    let ranked = ranking::rank_repos(
        vec![archived, repo("acme-org/api-v2", "2025-01-01T00:00:00Z")],
        "api",
    );

    assert_eq!(names(&ranked), ["acme-org/api-v2", "acme-org/api"]);
}

#[test]
fn an_empty_query_keeps_everything_by_recent_push() {
    let ranked = ranking::rank_repos(
        vec![
            repo("acme-org/a", "2025-01-01T00:00:00Z"),
            repo("acme-org/b", "2026-01-01T00:00:00Z"),
        ],
        "  ",
    );

    assert_eq!(names(&ranked), ["acme-org/b", "acme-org/a"]);
}

#[test]
fn the_owner_is_not_matched() {
    assert!(
        ranking::rank_repos(vec![repo("acme-org/web", "2026-01-01T00:00:00Z")], "acme").is_empty()
    );
}
