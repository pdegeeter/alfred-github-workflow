//! Tests for the parsing of what follows each keyword.

use alfred_gh::query::{self, ReposQuery};

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
