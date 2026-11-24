//! Tests for repository filtering and ranking.

use alfred_gh::prs::Pr;
use alfred_gh::ranking::{self, Resolution};
use alfred_gh::repos::Repo;

fn pr(number: u64, title: &str, author: &str, head: &str) -> Pr {
    Pr {
        number,
        title: title.to_owned(),
        url: format!("https://github.com/acme-org/api/pull/{number}"),
        state: "OPEN".to_owned(),
        is_draft: false,
        updated_at: "2026-11-24T00:00:00Z".to_owned(),
        head: head.to_owned(),
        base: "main".to_owned(),
        review_decision: None,
        author: Some(author.to_owned()),
        repo: "acme-org/api".to_owned(),
        checks: None,
    }
}

fn catalog() -> Vec<Repo> {
    vec![
        repo("acme-org/api", "2026-01-02T00:00:00Z"),
        repo("octo-user/api", "2026-01-01T00:00:00Z"),
        repo("acme-org/web-app", "2026-01-03T00:00:00Z"),
    ]
}

#[test]
fn owner_slash_name_resolves_to_itself_with_github_casing() {
    assert_eq!(
        ranking::resolve_repo(&catalog(), "ACME-ORG/Api"),
        Resolution::Exact("acme-org/api".to_owned())
    );
    assert_eq!(
        ranking::resolve_repo(&catalog(), "other-org/tool"),
        Resolution::Exact("other-org/tool".to_owned()),
        "repositories outside the owners still work"
    );
    assert_eq!(
        ranking::resolve_repo(&catalog(), "-x/y"),
        Resolution::Unknown
    );
}

#[test]
fn a_bare_name_resolves_only_when_unique() {
    assert_eq!(
        ranking::resolve_repo(&catalog(), "web-app"),
        Resolution::Exact("acme-org/web-app".to_owned())
    );

    let Resolution::Candidates(candidates) = ranking::resolve_repo(&catalog(), "api") else {
        panic!("api is ambiguous");
    };
    assert_eq!(names(&candidates), ["acme-org/api", "octo-user/api"]);
}

#[test]
fn partial_and_empty_names_list_candidates_and_unknown_says_so() {
    let Resolution::Candidates(partial) = ranking::resolve_repo(&catalog(), "web") else {
        panic!("web is partial");
    };
    assert_eq!(names(&partial), ["acme-org/web-app"]);

    let Resolution::Candidates(all) = ranking::resolve_repo(&catalog(), "") else {
        panic!("nothing typed lists everything");
    };
    assert_eq!(all.len(), 3);

    assert_eq!(
        ranking::resolve_repo(&catalog(), "zzz"),
        Resolution::Unknown
    );
}

#[test]
fn pull_requests_filter_on_every_word() {
    let prs = vec![
        pr(42, "Fix login redirect", "octo-user", "fix/login"),
        pr(7, "Add billing export", "dev-two", "feat/PAY-9-export"),
    ];
    let numbers = |text: &str| -> Vec<u64> {
        ranking::filter_prs(prs.clone(), text)
            .iter()
            .map(|pr| pr.number)
            .collect()
    };

    assert_eq!(numbers(""), [42, 7]);
    assert_eq!(numbers("LOGIN"), [42]);
    assert_eq!(numbers("dev-two"), [7]);
    assert_eq!(numbers("pay-9"), [7]);
    assert_eq!(numbers("#7"), [7]);
    assert_eq!(numbers("7"), [7]);
    assert_eq!(numbers("fix billing"), Vec::<u64>::new());
}

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
