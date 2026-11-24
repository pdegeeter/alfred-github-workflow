//! Tests for the parsing of what follows each keyword.

use alfred_gh::prs::PrState;
use alfred_gh::query::{self, RepoQuery, ReposQuery};

#[test]
fn ghp_gha_split_the_repository_from_the_rest() {
    let split = |input: &str| {
        let RepoQuery { repo, rest } = query::parse_repo(input);

        (repo, rest)
    };

    assert_eq!(
        split("acme-org/api fix  login "),
        ("acme-org/api".to_owned(), "fix  login".to_owned())
    );
    assert_eq!(split("api"), ("api".to_owned(), String::new()));
    assert_eq!(split("  api "), ("api".to_owned(), String::new()));
    assert_eq!(split(""), (String::new(), String::new()));
}

#[test]
fn ghp_state_filter_defaults_to_open() {
    assert_eq!(
        query::parse_pr_filter("fix login"),
        (PrState::Open, "fix login".to_owned())
    );
    assert_eq!(
        query::parse_pr_filter("state:merged fix"),
        (PrState::Merged, "fix".to_owned())
    );
    assert_eq!(
        query::parse_pr_filter("fix state:ALL"),
        (PrState::All, "fix".to_owned())
    );
    assert_eq!(
        query::parse_pr_filter("state:weird"),
        (PrState::Open, "state:weird".to_owned())
    );
}

fn owners() -> Vec<String> {
    vec!["acme-org".to_owned(), "octo-user".to_owned()]
}

fn repos_query(owner: Option<&str>, text: &str) -> ReposQuery {
    ReposQuery {
        owner: owner.map(str::to_owned),
        text: text.to_owned(),
    }
}

#[test]
fn gho_plain_text_searches_every_owner() {
    assert_eq!(
        query::parse_repos("  api ", &owners()),
        repos_query(None, "api")
    );
    assert_eq!(query::parse_repos("", &owners()), repos_query(None, ""));
}

#[test]
fn gho_owner_prefix_restricts_the_list() {
    assert_eq!(
        query::parse_repos("a:api", &owners()),
        repos_query(Some("acme-org"), "api")
    );
    assert_eq!(
        query::parse_repos("o: dot", &owners()),
        repos_query(Some("octo-user"), "dot")
    );
    assert_eq!(
        query::parse_repos("octo-user:", &owners()),
        repos_query(Some("octo-user"), "")
    );
}

#[test]
fn gho_unknown_prefixes_are_plain_text() {
    assert_eq!(
        query::parse_repos("z:api", &owners()),
        repos_query(None, "z:api")
    );
    assert_eq!(
        query::parse_repos("fix bug: x", &owners()),
        repos_query(None, "fix bug: x")
    );
}
