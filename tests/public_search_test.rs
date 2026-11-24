//! Tests for the `gh` keyword: live public repository search.

mod common;

use alfred_gh::repos::SEARCH_REPOS_QUERY;
use common::{app, items_json, FakeRunner, GH, NOW};

const SEARCH: &str = include_str!("fixtures/search_repos.json");

#[test]
fn an_empty_query_shows_a_hint_without_calling_gh() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new();

    let items = items_json(&app(&runner, dir.path(), NOW, "").search_repos("  ").items);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Search GitHub repositories");
    assert_eq!(items[0]["valid"], false);
    assert!(runner.calls().is_empty());
}

#[test]
fn the_query_is_passed_as_is_and_results_are_repo_items() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, SEARCH);
    let app = app(&runner, dir.path(), NOW, "");

    let listing = app.search_repos("alfred workflow language:rust");
    let items = items_json(&listing.items);

    assert_eq!(
        runner.calls()[0].args,
        [
            "api",
            "graphql",
            "-f",
            &format!("query={SEARCH_REPOS_QUERY}"),
            "-f",
            "q=alfred workflow language:rust"
        ]
    );
    assert!(!listing.rerun);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "public-org/alfred-kit");
    assert_eq!(
        items[0]["arg"],
        "open https://github.com/public-org/alfred-kit"
    );
    assert_eq!(
        items[0]["mods"]["cmd"]["arg"],
        "clone-open public-org/alfred-kit Visual Studio Code"
    );
    assert_eq!(
        items[0]["mods"]["ctrl"]["arg"],
        "reopen ghp public-org/alfred-kit "
    );
}

#[test]
fn nothing_is_cached() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, SEARCH);

    app(&runner, dir.path(), NOW, "").search_repos("alfred");

    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[test]
fn no_result_and_errors_are_items() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, r#"{"data":{"search":{"nodes":[]}}}"#);

    let items = items_json(&app(&runner, dir.path(), NOW, "").search_repos("zzz").items);
    assert_eq!(items[0]["title"], "No repository matches “zzz”");

    let runner = FakeRunner::new().with_failure(GH, "HTTP 401: Bad credentials");
    let items = items_json(&app(&runner, dir.path(), NOW, "").search_repos("zzz").items);
    assert_eq!(items[0]["title"], "GitHub CLI is not authenticated");
}
