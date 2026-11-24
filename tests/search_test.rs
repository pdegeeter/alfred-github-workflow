//! Tests for `ghr` (review requested from me) and `ghm` (my pull requests):
//! the search query, its owner qualifiers and the listing.

mod common;

use alfred_gh::cache::{Entry, Key};
use alfred_gh::prs::{self, Pr, Search, SEARCH_PRS_QUERY};
use common::{app, items_json, titles, FakeRunner, GH, NOW};
use serde_json::Value;

const SEARCH: &str = include_str!("fixtures/search_prs.json");
const ORG: &str = include_str!("fixtures/repos_org.json");
const USER_PAGE_1: &str = include_str!("fixtures/repos_user_page1.json");
const USER_PAGE_2: &str = include_str!("fixtures/repos_user_page2.json");

#[test]
fn search_queries_name_who_and_where() {
    assert_eq!(
        Search::Review.query("org:acme-org user:octo-user"),
        "is:pr is:open archived:false review-requested:@me sort:updated-desc org:acme-org user:octo-user"
    );
    assert_eq!(
        Search::Mine.query(""),
        "is:pr is:open archived:false author:@me sort:updated-desc"
    );
}

#[test]
fn search_results_parse_as_pull_requests() {
    let body: Value = serde_json::from_str(SEARCH).unwrap();
    let parsed = prs::parse_search(&body["data"]);

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[2].repo, "octo-user/dotfiles");
}

#[test]
fn ghr_builds_qualifiers_from_the_owner_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2)
        .with_success(GH, SEARCH);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let items = items_json(&app.search(Search::Review, "").items);

    let search_call = &runner.calls()[3];
    assert_eq!(
        search_call.args,
        [
            "api",
            "graphql",
            "-f",
            &format!("query={SEARCH_PRS_QUERY}"),
            "-f",
            "q=is:pr is:open archived:false review-requested:@me sort:updated-desc org:acme-org user:octo-user"
        ]
    );
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0]["title"],
        "acme-org/api · [PAY-1234] Fix rounding of invoice totals"
    );
    assert_eq!(items[2]["title"], "octo-user/dotfiles · Tidy shell aliases");
}

#[test]
fn ghm_searches_my_pull_requests_and_caches_them_per_qualifiers() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, SEARCH);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    app.search(Search::Mine, "");

    assert!(runner.calls()[1].args[5].contains("author:@me"));
    assert!(matches!(
        app.cache.read::<Vec<Pr>>(&Key::Search {
            search: Search::Mine,
            qualifiers: "org:acme-org".to_owned()
        }),
        Entry::Fresh(_)
    ));
}

#[test]
fn the_query_filters_the_results() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, SEARCH);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    assert_eq!(
        titles(&app.search(Search::Review, "aliases").items),
        ["octo-user/dotfiles · Tidy shell aliases"]
    );
}

#[test]
fn a_failing_owner_is_reported_and_left_out_of_the_qualifiers() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_failure(GH, "gh: boom")
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2)
        .with_success(GH, SEARCH);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let items = items_json(&app.search(Search::Review, "").items);

    assert_eq!(items[0]["title"], "acme-org: GitHub error");
    assert!(runner.calls()[3].args[5].ends_with("sort:updated-desc user:octo-user"));
}

#[test]
fn nothing_to_review_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, r#"{"data":{"search":{"nodes":[]}}}"#);

    let items = items_json(
        &app(&runner, dir.path(), NOW, "acme-org")
            .search(Search::Review, "")
            .items,
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "No pull request waiting for your review");
    assert_eq!(items[0]["subtitle"], "Searched org:acme-org");
}

#[test]
fn search_keys_round_trip_with_their_qualifiers() {
    let key = Key::Search {
        search: Search::Review,
        qualifiers: "org:acme-org user:octo-user".to_owned(),
    };

    assert_eq!(key.to_arg(), "search review org:acme-org user:octo-user");
    assert_eq!(Key::parse(&key.to_arg()).unwrap(), key);
    assert_eq!(key.stem(), "review-org_acme-org_user_octo-user");
    assert_eq!(key.ttl().as_secs(), 60);
}
