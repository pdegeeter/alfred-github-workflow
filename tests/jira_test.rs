//! Tests for Jira ticket key extraction.

use alfred_gh::jira;

#[test]
fn finds_keys_in_titles_and_branches() {
    assert_eq!(
        jira::find_key("[PAY-1234] Fix rounding").as_deref(),
        Some("PAY-1234")
    );
    assert_eq!(
        jira::find_key("feat/OPS-77-webhook").as_deref(),
        Some("OPS-77")
    );
    assert_eq!(
        jira::find_key("A1B2-9 at the start").as_deref(),
        Some("A1B2-9")
    );
    assert_eq!(jira::find_key("ends with AB-1").as_deref(), Some("AB-1"));
}

#[test]
fn ignores_what_is_not_a_key() {
    for text in [
        "",
        "no ticket here",
        "lowercase abc-123",
        "single letter A-123",
        "no digits PAY-",
        "digit first 1AB-2",
        "UTF8 has no dash",
    ] {
        assert_eq!(jira::find_key(text), None, "{text:?}");
    }
}

#[test]
fn a_key_does_not_start_inside_an_uppercase_word() {
    assert_eq!(jira::find_key("XPAY-12").as_deref(), Some("XPAY-12"));
    assert_eq!(jira::find_key("9PAY-12"), None);
}

#[test]
fn the_title_wins_over_the_branch() {
    assert_eq!(
        jira::pr_key("[PAY-1] Fix", "feat/OPS-2-x").as_deref(),
        Some("PAY-1")
    );
    assert_eq!(
        jira::pr_key("Fix", "feat/OPS-2-x").as_deref(),
        Some("OPS-2")
    );
    assert_eq!(jira::pr_key("Fix", "fix/thing"), None);
}
