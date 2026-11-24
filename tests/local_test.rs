//! Tests for local clone paths.

use std::path::Path;

use alfred_gh::local;

#[test]
fn clones_live_under_workspace_github_com() {
    assert_eq!(
        local::clone_path(Path::new("/Users/octo"), "acme-org/api"),
        Path::new("/Users/octo/Workspace/github.com/acme-org/api")
    );
}

#[test]
fn only_plain_owner_slash_repo_names_are_valid() {
    for valid in ["acme-org/api", "octo-user/my.repo_2", "a/b"] {
        assert!(local::is_valid_name_with_owner(valid), "{valid:?}");
    }

    for invalid in [
        "acme-org",
        "/api",
        "acme-org/",
        "-x/api",
        "acme-org/-api",
        "acme-org/..",
        "acme-org/.git",
        "a/b/c",
        "acme org/api",
        "acme-org/api;rm",
    ] {
        assert!(!local::is_valid_name_with_owner(invalid), "{invalid:?}");
    }
}
