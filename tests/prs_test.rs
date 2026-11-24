//! Tests for pull request parsing and the `ghp` script filter (repository
//! resolution, states, filters, cache).

mod common;

use alfred_gh::cache::{Entry, Key};
use alfred_gh::gh::Gh;
use alfred_gh::prs::{self, Checks, Pr, PrState, ALL_PRS_QUERY, MERGED_PRS_QUERY, OPEN_PRS_QUERY};
use common::{app, items_json, titles, FakeRunner, GH, NOW};
use serde_json::Value;

const PRS: &str = include_str!("fixtures/prs_repo.json");
const ORG: &str = include_str!("fixtures/repos_org.json");
const USER_WITH_API: &str = r#"{"data":{"repositoryOwner":{"__typename":"User","repositories":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[{"nameWithOwner":"octo-user/api","description":"Toy API","url":"https://github.com/octo-user/api","homepageUrl":null,"pushedAt":"2026-01-01T00:00:00Z","isArchived":false,"isPrivate":false,"isFork":false,"primaryLanguage":null,"repositoryTopics":{"nodes":[]},"defaultBranchRef":{"name":"main"}}]}}}}"#;

fn parsed() -> Vec<Pr> {
    let body: Value = serde_json::from_str(PRS).unwrap();

    prs::parse_repo_prs(&body["data"]).unwrap()
}

#[test]
fn pull_requests_parse_every_field() {
    let prs = parsed();

    assert_eq!(prs.len(), 4);

    let first = &prs[0];
    assert_eq!(first.number, 42);
    assert_eq!(first.title, "[PAY-1234] Fix rounding of invoice totals");
    assert_eq!(first.repo, "acme-org/api");
    assert_eq!(first.author.as_deref(), Some("octo-user"));
    assert_eq!(
        (first.head.as_str(), first.base.as_str()),
        ("fix/invoice-rounding", "main")
    );
    assert_eq!(first.checks, Some(Checks::Success));
    assert_eq!(first.review_label(), Some("approved"));
    assert_eq!(
        first.files_url(),
        "https://github.com/acme-org/api/pull/42/files"
    );

    assert!(prs[1].is_draft);
    assert_eq!(prs[1].checks, Some(Checks::Pending));
    assert_eq!(prs[1].review_label(), Some("review required"));
    assert_eq!(prs[2].checks, Some(Checks::Failure));
    assert_eq!(prs[2].review_label(), Some("changes requested"));

    let ghost = &prs[3];
    assert_eq!(ghost.author, None);
    assert_eq!(ghost.checks, None);
    assert_eq!(ghost.state, "MERGED");
}

#[test]
fn an_unknown_repository_has_no_pull_requests() {
    assert_eq!(
        prs::parse_repo_prs(&serde_json::json!({ "repository": null })),
        None
    );
}

#[test]
fn each_state_uses_its_own_fixed_query() {
    assert!(OPEN_PRS_QUERY.contains("states: [OPEN]"));
    assert!(MERGED_PRS_QUERY.contains("states: [MERGED]"));
    assert!(!ALL_PRS_QUERY.contains("states:"));

    for (state, query) in [
        (PrState::Open, OPEN_PRS_QUERY),
        (PrState::Merged, MERGED_PRS_QUERY),
        (PrState::All, ALL_PRS_QUERY),
    ] {
        let runner = FakeRunner::new().with_success(GH, PRS);

        prs::fetch_repo_prs(&Gh::new(&runner, Some(GH)), "acme-org/api", state).unwrap();

        assert_eq!(
            runner.calls()[0].args,
            [
                "api",
                "graphql",
                "-f",
                &format!("query={query}"),
                "-f",
                "owner=acme-org",
                "-f",
                "name=api"
            ]
        );
    }
}

#[test]
fn ghp_with_owner_slash_name_lists_open_pull_requests_without_listing_repos() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, PRS);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let listing = app.prs("acme-org/api ");
    let items = items_json(&listing.items);

    assert_eq!(runner.calls().len(), 1, "only the pull request query");
    assert_eq!(items.len(), 4);
    assert_eq!(
        items[0]["title"],
        "[PAY-1234] Fix rounding of invoice totals"
    );
    assert!(matches!(
        app.cache.read::<Vec<Pr>>(&Key::Prs {
            repo: "acme-org/api".to_owned(),
            state: PrState::Open
        }),
        Entry::Fresh(_)
    ));
}

#[test]
fn ghp_resolves_a_bare_name_that_only_one_owner_has() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, PRS);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    let titles = titles(&app.prs("web-app").items);

    assert_eq!(titles.len(), 4);
    assert_eq!(runner.calls()[1].args[5], "owner=acme-org");
    assert_eq!(runner.calls()[1].args[7], "name=web-app");
}

#[test]
fn ghp_lists_candidates_for_an_ambiguous_name() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, USER_WITH_API);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let items = items_json(&app.prs("api").items);

    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["valid"], false);
    assert_eq!(items[0]["autocomplete"], "acme-org/api ");
    assert_eq!(items[1]["autocomplete"], "octo-user/api ");
}

#[test]
fn ghp_lists_candidates_for_a_partial_name_and_for_nothing_typed() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, ORG);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    assert_eq!(titles(&app.prs("leg").items), ["acme-org/legacy-tools"]);
    assert_eq!(titles(&app.prs("").items).len(), 3);
}

#[test]
fn ghp_says_when_no_repository_matches() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, ORG);

    let items = items_json(&app(&runner, dir.path(), NOW, "acme-org").prs("zzz").items);

    assert_eq!(items[0]["title"], "No repository matches “zzz”");
    assert_eq!(items[0]["valid"], false);
}

#[test]
fn ghp_filters_by_title_author_branch_and_number() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, PRS);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    assert_eq!(titles(&app.prs("acme-org/api webhook").items).len(), 1);
    assert_eq!(titles(&app.prs("acme-org/api dev-two").items).len(), 1);
    assert_eq!(
        titles(&app.prs("acme-org/api dependabot/npm").items).len(),
        1
    );
    assert_eq!(
        titles(&app.prs("acme-org/api #42").items),
        ["[PAY-1234] Fix rounding of invoice totals"]
    );
    assert_eq!(
        titles(&app.prs("acme-org/api 41").items),
        ["Add retries to the webhook sender"]
    );
}

#[test]
fn ghp_state_switches_query_and_cache_entry() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, PRS);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    app.prs("acme-org/api state:merged");

    assert_eq!(
        runner.calls()[0].args[3],
        format!("query={MERGED_PRS_QUERY}")
    );
    assert!(matches!(
        app.cache.read::<Vec<Pr>>(&Key::Prs {
            repo: "acme-org/api".to_owned(),
            state: PrState::Merged
        }),
        Entry::Fresh(_)
    ));
}

#[test]
fn ghp_serves_stale_pull_requests_while_refreshing() {
    let dir = tempfile::tempdir().unwrap();
    let warm = FakeRunner::new().with_success(GH, PRS);
    app(&warm, dir.path(), NOW, "acme-org").prs("acme-org/api ");

    let runner = FakeRunner::new();
    let listing = app(&runner, dir.path(), NOW + 60, "acme-org").prs("acme-org/api ");

    assert!(listing.rerun);
    assert!(runner.calls().is_empty());
    assert_eq!(
        runner.spawns()[0].args,
        ["refresh", "prs acme-org/api open"]
    );
}

#[test]
fn ghp_reports_errors_and_empty_lists() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(
        GH,
        r#"{"data":{"repository":null},"errors":[{"type":"NOT_FOUND","message":"Could not resolve to a Repository with the name 'acme-org/nope'."}]}"#,
    );

    let items = items_json(
        &app(&runner, dir.path(), NOW, "acme-org")
            .prs("acme-org/nope ")
            .items,
    );
    assert_eq!(items[0]["title"], "acme-org/nope: GitHub error");

    let runner = FakeRunner::new().with_success(
        GH,
        r#"{"data":{"repository":{"pullRequests":{"nodes":[]}}}}"#,
    );
    let items = items_json(
        &app(&runner, dir.path(), NOW, "acme-org")
            .prs("acme-org/quiet ")
            .items,
    );
    assert_eq!(items[0]["title"], "No open pull request in acme-org/quiet");
}
