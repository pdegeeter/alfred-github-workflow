//! Tests for the JSON Alfred receives from script filters.

use std::time::Duration;

use alfred_gh::action::Action;
use alfred_gh::alfred;
use alfred_gh::gh::GhError;
use alfred_gh::prs::{Checks, Pr};
use alfred_gh::repos::Repo;
use serde_json::Value;

fn json(output: &powerpack::Output) -> Value {
    let mut buffer = Vec::new();
    output.write(&mut buffer).unwrap();

    serde_json::from_slice(&buffer).unwrap()
}

fn item_json(item: powerpack::Item) -> Value {
    serde_json::to_value(item).unwrap()
}

/// Every modifier `arg` of an item, keyed by Alfred's modifier name.
fn mod_args(item: &Value) -> Vec<(String, String)> {
    item["mods"]
        .as_object()
        .map(|mods| {
            mods.iter()
                .map(|(key, modifier)| {
                    (
                        key.clone(),
                        modifier["arg"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn repo() -> Repo {
    Repo {
        name_with_owner: "acme-org/api".to_owned(),
        description: Some("Public API service".to_owned()),
        url: "https://github.com/acme-org/api".to_owned(),
        homepage_url: None,
        pushed_at: Some("2026-11-22T00:00:00Z".to_owned()),
        is_archived: false,
        is_private: true,
        is_fork: false,
        language: Some("TypeScript".to_owned()),
        topics: Vec::new(),
        default_branch: Some("main".to_owned()),
    }
}

const NOW: i64 = 1_795_564_800; // 2026-11-25T00:00:00Z

#[test]
fn error_items_are_not_actionable() {
    let output = alfred::output(vec![alfred::error_item("Oops", "details")], None);
    let item = &json(&output)["items"][0];

    assert_eq!(item["title"], "Oops");
    assert_eq!(item["subtitle"], "details");
    assert_eq!(item["valid"], false);
    assert!(item.get("uid").is_none());
}

#[test]
fn rerun_is_only_set_when_asked() {
    let without = json(&alfred::output(Vec::new(), None));
    let with = json(&alfred::output(Vec::new(), Some(alfred::RERUN)));

    assert!(without.get("rerun").is_none());
    assert_eq!(with["rerun"], 0.5);
    assert_eq!(alfred::RERUN, Duration::from_millis(500));
}

#[test]
fn a_repo_item_opens_clones_and_copies() {
    let item = item_json(alfred::repo_item(&repo(), "Zed", NOW));

    assert_eq!(item["title"], "acme-org/api");
    assert_eq!(
        item["subtitle"],
        "🔒 · TypeScript · Public API service · pushed 3d ago"
    );
    assert_eq!(item["arg"], "open https://github.com/acme-org/api");
    assert!(item.get("uid").is_none());
    assert_eq!(
        mod_args(&item),
        [
            (
                "alt".to_owned(),
                "copy https://github.com/acme-org/api".to_owned()
            ),
            ("cmd".to_owned(), "clone-open acme-org/api Zed".to_owned()),
            ("ctrl".to_owned(), "reopen ghp acme-org/api ".to_owned()),
        ]
    );
}

fn pr() -> Pr {
    Pr {
        number: 41,
        title: "Add retries".to_owned(),
        url: "https://github.com/acme-org/api/pull/41".to_owned(),
        state: "OPEN".to_owned(),
        is_draft: true,
        updated_at: "2026-11-24T21:00:00Z".to_owned(),
        head: "feat/OPS-77-retries".to_owned(),
        base: "main".to_owned(),
        review_decision: Some("REVIEW_REQUIRED".to_owned()),
        author: Some("dev-two".to_owned()),
        repo: "acme-org/api".to_owned(),
        checks: Some(Checks::Pending),
    }
}

#[test]
fn a_pr_item_shows_its_state_and_opens_files_copies_and_jira() {
    let item = item_json(alfred::pr_item(&pr(), false, NOW));

    assert_eq!(item["title"], "Add retries");
    assert_eq!(
        item["subtitle"],
        "#41 · dev-two · feat/OPS-77-retries→main · draft · CI 🟡 · review required · updated 3h ago"
    );
    assert_eq!(item["arg"], "open https://github.com/acme-org/api/pull/41");
    assert_eq!(
        mod_args(&item),
        [
            (
                "alt".to_owned(),
                "copy https://github.com/acme-org/api/pull/41".to_owned()
            ),
            (
                "cmd".to_owned(),
                "open https://github.com/acme-org/api/pull/41/files".to_owned()
            ),
            ("ctrl".to_owned(), "jira OPS-77".to_owned()),
        ]
    );
    assert_eq!(item["mods"]["ctrl"]["subtitle"], "Open OPS-77 in Jira");
}

#[test]
fn a_pr_item_across_repositories_names_its_repository() {
    let mut merged = pr();
    merged.state = "MERGED".to_owned();
    merged.head = "feat/retries".to_owned();
    merged.author = None;
    merged.checks = None;
    merged.review_decision = None;

    let item = item_json(alfred::pr_item(&merged, true, NOW));

    assert_eq!(item["title"], "acme-org/api · Add retries");
    assert_eq!(
        item["subtitle"],
        "#41 · ghost · feat/retries→main · merged · updated 3h ago"
    );
    assert_eq!(item["mods"]["ctrl"]["arg"], "jira");
}

#[test]
fn a_candidate_completes_the_query() {
    let item = item_json(alfred::candidate_item(&repo(), NOW));

    assert_eq!(item["valid"], false);
    assert_eq!(item["autocomplete"], "acme-org/api ");
}

#[test]
fn archived_repos_say_so_first() {
    let mut archived = repo();
    archived.is_archived = true;
    archived.is_private = false;

    let item = item_json(alfred::repo_item(&archived, "Zed", NOW));

    assert!(item["subtitle"]
        .as_str()
        .unwrap()
        .starts_with("Archived · "));
}

#[test]
fn every_modifier_arg_decodes() {
    let items = [
        alfred::repo_item(&repo(), "Visual Studio Code", NOW),
        alfred::refresh_item(),
        alfred::sso_item("acme-org"),
        alfred::pr_item(&pr(), true, NOW),
    ];

    for item in items {
        let item = item_json(item);

        if let Some(arg) = item["arg"].as_str() {
            Action::parse(arg).unwrap();
        }

        for (key, arg) in mod_args(&item) {
            Action::parse(&arg).unwrap_or_else(|err| panic!("{key}: {arg:?}: {err}"));
        }
    }
}

#[test]
fn gh_errors_explain_their_fix() {
    let cases = [
        (GhError::Missing, "GitHub CLI not found"),
        (GhError::NotAuthenticated, "GitHub CLI is not authenticated"),
        (
            GhError::RateLimited { reset: None },
            "acme-org: GitHub API rate limit reached",
        ),
        (
            GhError::SsoRequired,
            "acme-org: token not authorized for SSO",
        ),
        (GhError::Other("boom".to_owned()), "acme-org: GitHub error"),
    ];

    for (error, title) in cases {
        let item = item_json(alfred::gh_error_item(Some("acme-org"), &error, NOW));

        assert_eq!(item["title"], title);
    }
}
