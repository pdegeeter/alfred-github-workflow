//! Tests for `gh` resolution, argv building and error classification.

mod common;

use std::path::Path;

use alfred_gh::gh::{self, Gh, GhError};
use common::{FakeRunner, GH};

#[test]
fn resolve_prefers_homebrew_on_apple_silicon() {
    assert_eq!(gh::resolve(&|_: &Path| true), Some("/opt/homebrew/bin/gh"));
    assert_eq!(
        gh::resolve(&|path: &Path| path == Path::new("/usr/local/bin/gh")),
        Some("/usr/local/bin/gh")
    );
    assert_eq!(gh::resolve(&|_: &Path| false), None);
}

#[test]
fn a_missing_gh_is_reported_without_running_anything() {
    let runner = FakeRunner::new();
    let gh = Gh::new(&runner, None);

    assert_eq!(
        gh.graphql("query { viewer { login } }", &[]),
        Err(GhError::Missing)
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn graphql_passes_the_query_and_variables_as_separate_raw_fields() {
    let runner = FakeRunner::new().with_success(GH, r#"{"data":{"ok":true}}"#);
    let gh = Gh::new(&runner, Some(GH));

    let data = gh
        .graphql(
            "query($q: String!) { x }",
            &[("q", "@/etc/passwd $(rm -rf)")],
        )
        .unwrap();

    assert_eq!(data["ok"], true);
    assert_eq!(
        runner.calls()[0].args,
        [
            "api",
            "graphql",
            "-f",
            "query=query($q: String!) { x }",
            "-f",
            "q=@/etc/passwd $(rm -rf)"
        ]
    );
}

#[test]
fn viewer_login_reads_the_login() {
    let runner = FakeRunner::new().with_success(GH, r#"{"data":{"viewer":{"login":"octo-user"}}}"#);

    assert_eq!(
        Gh::new(&runner, Some(GH)).viewer_login().unwrap(),
        "octo-user"
    );
}

#[test]
fn not_authenticated_is_detected_from_stderr() {
    for stderr in [
        "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable.",
        "HTTP 401: Bad credentials (https://api.github.com/graphql)\nTry authenticating with:  gh auth login",
    ] {
        assert_eq!(Gh::classify("", stderr), GhError::NotAuthenticated, "{stderr}");
    }
}

#[test]
fn saml_enforcement_is_detected_from_the_graphql_body() {
    let body = include_str!("fixtures/graphql_saml.json");

    assert_eq!(
        Gh::classify(
            body,
            "gh: Resource protected by organization SAML enforcement."
        ),
        GhError::SsoRequired
    );
}

#[test]
fn saml_errors_are_classified_even_when_gh_exits_successfully() {
    let runner = FakeRunner::new().with_success(GH, include_str!("fixtures/graphql_saml.json"));

    assert_eq!(
        Gh::new(&runner, Some(GH)).graphql("q", &[]),
        Err(GhError::SsoRequired)
    );
}

#[test]
fn a_rate_limit_looks_up_its_reset_time() {
    let runner = FakeRunner::new()
        .with_failure_output(
            GH,
            include_str!("fixtures/graphql_rate_limited.json"),
            "gh: API rate limit exceeded for user ID 1000001.",
        )
        .with_success(GH, include_str!("fixtures/rate_limit.json"));
    let gh = Gh::new(&runner, Some(GH));

    assert_eq!(
        gh.graphql("q", &[]),
        Err(GhError::RateLimited {
            reset: Some(1_764_016_200)
        })
    );
    assert_eq!(runner.calls()[1].args, ["api", "rate_limit"]);
}

#[test]
fn a_rate_limit_without_a_known_reset_still_says_so() {
    let runner = FakeRunner::new().with_failure(GH, "gh: API rate limit exceeded");

    assert_eq!(
        Gh::new(&runner, Some(GH)).rest("rate_limit"),
        Err(GhError::RateLimited { reset: None })
    );
}

#[test]
fn other_errors_keep_their_message() {
    assert_eq!(
        Gh::classify("", "gh: Could not resolve to a Repository\n"),
        GhError::Other("Could not resolve to a Repository".to_owned())
    );
    assert_eq!(
        Gh::classify(
            r#"{"errors":[{"type":"NOT_FOUND","message":"Not found"}]}"#,
            ""
        ),
        GhError::Other("Not found".to_owned())
    );
}

#[test]
fn rest_passes_the_path_only() {
    let runner = FakeRunner::new().with_success(GH, r#"{"total_count":0}"#);

    let body = Gh::new(&runner, Some(GH))
        .rest("repos/acme-org/api/actions/workflows")
        .unwrap();

    assert_eq!(body["total_count"], 0);
    assert_eq!(
        runner.calls()[0].args,
        ["api", "repos/acme-org/api/actions/workflows"]
    );
}
