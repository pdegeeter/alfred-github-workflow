//! Tests for repository fetching (parsing, pagination) and the `gho` script
//! filter (owners, cache, partial failures, SSO).

mod common;

use alfred_gh::cache::{Cache, Entry, Key};
use alfred_gh::gh::{Gh, GhError};
use alfred_gh::owners::OwnerKind;
use alfred_gh::repos::{self, OwnerRepos, REPOS_QUERY};
use common::{app, at, items_json, titles, FakeRunner, EXE, GH, NOW};
use serde_json::Value;

const USER_PAGE_1: &str = include_str!("fixtures/repos_user_page1.json");
const USER_PAGE_2: &str = include_str!("fixtures/repos_user_page2.json");
const ORG: &str = include_str!("fixtures/repos_org.json");
const SAML: &str = include_str!("fixtures/graphql_saml.json");
const EMPTY_ORG: &str = r#"{"data":{"repositoryOwner":{"__typename":"Organization","repositories":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}}}}"#;

fn data(body: &str) -> Value {
    serde_json::from_str::<Value>(body).unwrap()["data"].clone()
}

#[test]
fn a_page_parses_every_field() {
    let page = repos::parse_page(&data(USER_PAGE_1)).unwrap();

    assert_eq!(page.kind, OwnerKind::User);
    assert_eq!(page.next.as_deref(), Some("Y3Vyc29yOnBhZ2Ux"));
    assert_eq!(page.repos.len(), 3);

    let first = &page.repos[0];
    assert_eq!(first.name_with_owner, "octo-user/alfred-tools");
    assert_eq!(first.owner(), "octo-user");
    assert_eq!(first.name(), "alfred-tools");
    assert_eq!(first.url, "https://github.com/octo-user/alfred-tools");
    assert_eq!(first.topics, ["alfred", "rust"]);
    assert_eq!(first.language, None);
    assert_eq!(first.default_branch.as_deref(), Some("main"));
    assert!(!first.is_private);

    let dotfiles = &page.repos[1];
    assert!(dotfiles.is_private);
    assert_eq!(dotfiles.description, None);
    assert_eq!(dotfiles.language.as_deref(), Some("Shell"));
}

#[test]
fn empty_strings_are_absent_values() {
    let page = repos::parse_page(&data(USER_PAGE_2)).unwrap();
    let fork = &page.repos[0];

    assert!(fork.is_fork);
    assert_eq!(fork.homepage_url, None, "an empty homepage is no homepage");
    assert_eq!(page.next, None);
}

#[test]
fn an_unknown_owner_has_no_page() {
    assert_eq!(
        repos::parse_page(&data(r#"{"data":{"repositoryOwner":null}}"#)),
        None
    );
}

#[test]
fn fetch_follows_the_cursor_and_merges_pages() {
    let runner = FakeRunner::new()
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2);

    let owner = repos::fetch_owner(&Gh::new(&runner, Some(GH)), "octo-user").unwrap();

    assert_eq!(owner.login, "octo-user");
    assert_eq!(owner.kind, OwnerKind::User);
    assert_eq!(owner.repos.len(), 6);

    let calls = runner.calls();
    assert_eq!(
        calls[0].args,
        [
            "api",
            "graphql",
            "-f",
            &format!("query={REPOS_QUERY}"),
            "-f",
            "login=octo-user"
        ]
    );
    assert_eq!(
        calls[1].args[4..],
        ["-f", "login=octo-user", "-f", "after=Y3Vyc29yOnBhZ2Ux"]
    );
}

#[test]
fn fetch_of_an_unknown_owner_fails_readably() {
    let runner = FakeRunner::new().with_success(GH, r#"{"data":{"repositoryOwner":null}}"#);

    assert_eq!(
        repos::fetch_owner(&Gh::new(&runner, Some(GH)), "nobody"),
        Err(GhError::Other(
            "nobody: no such organization or user".to_owned()
        ))
    );
}

#[test]
fn gho_lists_every_owner_ranked_with_the_refresh_item_last() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let listing = app.repos("");
    let titles = titles(&listing.items);

    assert!(!listing.rerun);
    assert_eq!(titles.len(), 3 + 6 + 1);
    assert_eq!(titles.last().unwrap(), "Refresh cache");
    assert_eq!(
        titles[titles.len() - 2],
        "acme-org/legacy-tools",
        "archived last"
    );
}

#[test]
fn gho_writes_each_owner_to_its_own_cache_entry() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2);
    app(&runner, dir.path(), NOW, "acme-org,octo-user").repos("");

    let cache = Cache::new(dir.path(), at(NOW));
    let Entry::Fresh(org) = cache.read::<OwnerRepos>(&Key::Repos {
        owner: "acme-org".to_owned(),
    }) else {
        panic!("acme-org should be cached");
    };

    assert_eq!(org.kind, OwnerKind::Org);
    assert_eq!(org.repos.len(), 3);
}

#[test]
fn gho_serves_a_fresh_cache_without_calling_gh() {
    let dir = tempfile::tempdir().unwrap();
    let warm = FakeRunner::new().with_success(GH, ORG);
    app(&warm, dir.path(), NOW, "acme-org").repos("");

    let runner = FakeRunner::new();
    let listing = app(&runner, dir.path(), NOW + 60, "acme-org").repos("api");

    assert!(runner.calls().is_empty());
    assert!(runner.spawns().is_empty());
    assert!(!listing.rerun);
    assert_eq!(titles(&listing.items), ["acme-org/api"]);
}

#[test]
fn gho_serves_a_stale_cache_and_refreshes_it_in_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let warm = FakeRunner::new().with_success(GH, ORG);
    app(&warm, dir.path(), NOW, "acme-org").repos("");

    let runner = FakeRunner::new();
    let listing = app(&runner, dir.path(), NOW + 3_600, "acme-org").repos("api");

    assert!(runner.calls().is_empty(), "no synchronous fetch");
    assert!(listing.rerun);
    assert_eq!(titles(&listing.items), ["acme-org/api"]);

    let spawns = runner.spawns();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].program, EXE);
    assert_eq!(spawns[0].args, ["refresh", "repos acme-org"]);
}

#[test]
fn gho_does_not_spawn_a_second_refresh_while_one_runs() {
    let dir = tempfile::tempdir().unwrap();
    let warm = FakeRunner::new().with_success(GH, ORG);
    app(&warm, dir.path(), NOW, "acme-org").repos("");
    let key = Key::Repos {
        owner: "acme-org".to_owned(),
    };
    let _lock = Cache::new(dir.path(), at(NOW + 3_600))
        .try_lock(&key)
        .unwrap()
        .unwrap();

    let runner = FakeRunner::new();
    let listing = app(&runner, dir.path(), NOW + 3_610, "acme-org").repos("");

    assert!(runner.spawns().is_empty());
    assert!(listing.rerun, "keep rerunning until the refresh lands");
}

#[test]
fn refresh_rewrites_the_entry_and_releases_its_lock() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, ORG);
    let app = app(&runner, dir.path(), NOW, "acme-org");
    let key = Key::Repos {
        owner: "acme-org".to_owned(),
    };

    app.refresh(&key).unwrap();

    assert!(matches!(
        app.cache.read::<OwnerRepos>(&key),
        Entry::Fresh(_)
    ));
    assert!(!app.cache.is_locked(&key));
}

#[test]
fn a_failed_refresh_keeps_its_lock_to_back_off() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_failure(GH, "gh: something broke");
    let app = app(&runner, dir.path(), NOW, "acme-org");
    let key = Key::Repos {
        owner: "acme-org".to_owned(),
    };

    assert!(app.refresh(&key).is_err());
    assert!(app.cache.is_locked(&key));
}

#[test]
fn a_refresh_already_running_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new();
    let app = app(&runner, dir.path(), NOW, "acme-org");
    let key = Key::Viewer;
    let _lock = app.cache.try_lock(&key).unwrap().unwrap();

    app.refresh(&key).unwrap();

    assert!(runner.calls().is_empty());
}

#[test]
fn one_failing_owner_does_not_hide_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_failure(GH, "gh: Something went wrong")
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let items = items_json(&app.repos("").items);

    assert_eq!(items[0]["title"], "acme-org: GitHub error");
    assert_eq!(items[0]["subtitle"], "Something went wrong");
    assert_eq!(items[0]["valid"], false);
    assert_eq!(items[1]["title"], "octo-user/alfred-tools");
}

#[test]
fn an_sso_error_offers_to_authorize_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_failure_output(
            GH,
            SAML,
            "gh: Resource protected by organization SAML enforcement.",
        )
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let items = items_json(&app.repos("").items);

    assert_eq!(items[0]["title"], "acme-org: token not authorized for SSO");
    assert_eq!(
        items[0]["arg"],
        "open https://github.com/settings/connections/applications"
    );
}

#[test]
fn an_empty_organization_next_to_a_working_owner_means_sso() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, EMPTY_ORG)
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let items = items_json(&app.repos("").items);

    assert_eq!(items[0]["title"], "acme-org: token not authorized for SSO");
    assert!(
        matches!(
            app.cache.read::<OwnerRepos>(&Key::Repos {
                owner: "acme-org".to_owned()
            }),
            Entry::Missing
        ),
        "an empty organization is not cached"
    );
}

#[test]
fn gh_missing_and_not_authenticated_explain_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new();
    let mut missing = app(&runner, dir.path(), NOW, "acme-org,octo-user");
    missing.gh_path = None;

    let items = items_json(&missing.repos("").items);
    assert_eq!(items.len(), 1, "shown once, not per owner");
    assert_eq!(items[0]["title"], "GitHub CLI not found");
    assert_eq!(
        items[0]["subtitle"],
        "Install it with: brew install gh, then run: gh auth login"
    );

    let runner = FakeRunner::new().with_failure(
        GH,
        "To get started with GitHub CLI, please run:  gh auth login",
    );
    let items = items_json(&app(&runner, dir.path(), NOW, "acme-org").repos("").items);
    assert_eq!(items[0]["title"], "GitHub CLI is not authenticated");
    assert_eq!(items[0]["subtitle"], "Run: gh auth login");
}

#[test]
fn a_rate_limit_says_when_it_resets() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_failure(GH, "gh: API rate limit exceeded")
        .with_success(GH, include_str!("fixtures/rate_limit.json"));
    let app = app(&runner, dir.path(), 1_764_016_200 - 600, "acme-org");

    let items = items_json(&app.repos("").items);

    assert_eq!(items[0]["title"], "acme-org: GitHub API rate limit reached");
    assert_eq!(items[0]["subtitle"], "Resets in 10m 00s");
}

#[test]
fn without_gh_owners_the_authenticated_user_is_listed() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, r#"{"data":{"viewer":{"login":"Octo-User"}}}"#)
        .with_success(GH, USER_PAGE_1)
        .with_success(GH, USER_PAGE_2);
    let app = app(&runner, dir.path(), NOW, "");

    let titles = titles(&app.repos("dot").items);

    assert_eq!(titles, ["octo-user/dotfiles"]);
    assert_eq!(runner.calls()[1].args[5], "login=octo-user");
    assert!(matches!(
        app.cache.read::<String>(&Key::Viewer),
        Entry::Fresh(_)
    ));
}

#[test]
fn an_owner_prefix_only_fetches_that_owner() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, ORG);
    let app = app(&runner, dir.path(), NOW, "acme-org,octo-user");

    let titles = titles(&app.repos("a:web").items);

    assert_eq!(titles, ["acme-org/web-app"]);
    assert_eq!(runner.calls().len(), 1);
}

#[test]
fn no_match_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, ORG);

    let items = items_json(&app(&runner, dir.path(), NOW, "acme-org").repos("zzz").items);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "No matching repository");
    assert_eq!(items[0]["valid"], false);
}
