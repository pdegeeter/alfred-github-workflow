//! Tests for `GH_OWNERS` parsing, owner kinds, search qualifiers and owner
//! prefixes.

use alfred_gh::owners::{self, OwnerKind};

fn logins(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn parse_trims_lowercases_and_dedupes_in_order() {
    assert_eq!(
        owners::parse(" Acme-Org, octo-user ,,acme-org"),
        logins(&["acme-org", "octo-user"])
    );
    assert!(owners::parse("").is_empty());
    assert!(owners::parse(" , ").is_empty());
}

#[test]
fn kinds_come_from_the_graphql_typename() {
    assert_eq!(
        OwnerKind::from_typename("Organization"),
        Some(OwnerKind::Org)
    );
    assert_eq!(OwnerKind::from_typename("User"), Some(OwnerKind::User));
    assert_eq!(OwnerKind::from_typename("Bot"), None);
}

#[test]
fn search_qualifiers_follow_each_owner_kind() {
    let owners = vec![
        ("acme-org".to_owned(), OwnerKind::Org),
        ("octo-user".to_owned(), OwnerKind::User),
    ];

    assert_eq!(
        owners::search_qualifiers(&owners),
        "org:acme-org user:octo-user"
    );
    assert_eq!(owners::search_qualifiers(&[]), "");
}

#[test]
fn a_prefix_is_the_first_letter_or_the_full_login() {
    let owners = logins(&["acme-org", "octo-user"]);

    assert_eq!(owners::resolve_prefix("a", &owners), Some(&owners[0]));
    assert_eq!(owners::resolve_prefix("O", &owners), Some(&owners[1]));
    assert_eq!(
        owners::resolve_prefix("octo-user", &owners),
        Some(&owners[1])
    );
    assert_eq!(owners::resolve_prefix("z", &owners), None);
    assert_eq!(owners::resolve_prefix("ac", &owners), None);
    assert_eq!(owners::resolve_prefix("", &owners), None);
}

#[test]
fn a_shared_first_letter_needs_the_full_login() {
    let owners = logins(&["acme-org", "alpha-user"]);

    assert_eq!(owners::resolve_prefix("a", &owners), None);
    assert_eq!(
        owners::resolve_prefix("alpha-user", &owners),
        Some(&owners[1])
    );
}
