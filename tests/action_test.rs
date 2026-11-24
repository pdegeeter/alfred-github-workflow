//! Tests for the `arg` protocol and the execution of every action.

mod common;

use std::path::Path;
use std::time::SystemTime;

use alfred_gh::action::{self, Action, Context, OPEN, OSASCRIPT};
use alfred_gh::cache::{Cache, Key};
use common::{FakeRunner, GH};

fn every_action() -> Vec<Action> {
    vec![
        Action::Open {
            url: "https://github.com/acme-org/api/pull/42".to_owned(),
        },
        Action::Copy {
            url: "https://github.com/octo-user/dotfiles".to_owned(),
        },
        Action::CloneOpen {
            repo: "acme-org/api".to_owned(),
            editor: "Visual Studio Code".to_owned(),
        },
        Action::Refresh,
        Action::Reopen {
            query: "ghp acme-org/api ".to_owned(),
        },
        Action::Jira {
            key: Some("PAY-1234".to_owned()),
        },
        Action::Jira { key: None },
    ]
}

const JIRA_URL: &str = "https://jira.example.com/browse/";

fn run_with(
    runner: &FakeRunner,
    action: &Action,
    exists: bool,
    jira_url: &str,
) -> anyhow::Result<Option<String>> {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path(), SystemTime::now());
    let ctx = Context {
        runner,
        gh: Some(GH),
        home: "/Users/octo".into(),
        cache: &cache,
        exists: &|_: &Path| exists,
        jira_url,
    };

    action::execute(&ctx, action)
}

fn run(runner: &FakeRunner, action: &Action, exists: bool) -> anyhow::Result<Option<String>> {
    run_with(runner, action, exists, JIRA_URL)
}

#[test]
fn every_action_round_trips_through_its_arg() {
    for expected in every_action() {
        let arg = expected.to_arg();

        assert_eq!(Action::parse(&arg).unwrap(), expected, "arg: {arg:?}");
    }
}

#[test]
fn args_start_with_their_verb() {
    let verbs: Vec<String> = every_action()
        .iter()
        .map(|action| action.to_arg().split(' ').next().unwrap().to_owned())
        .collect();

    assert_eq!(
        verbs,
        [
            "open",
            "copy",
            "clone-open",
            "refresh",
            "reopen",
            "jira",
            "jira"
        ]
    );
}

#[test]
fn parse_rejects_unknown_verbs_and_missing_payloads() {
    for arg in [
        "explode https://github.com",
        "open",
        "copy   ",
        "",
        "clone-open acme-org/api",
        "refresh",
        "refresh some",
    ] {
        assert!(Action::parse(arg).is_err(), "{arg:?} should not parse");
    }
}

#[test]
fn open_hands_the_url_to_macos_open() {
    let runner = FakeRunner::new().with_success(OPEN, "");
    let action = Action::Open {
        url: "https://github.com/acme-org/api".to_owned(),
    };

    let message = run(&runner, &action, true).unwrap();

    assert_eq!(message, None);
    assert_eq!(runner.calls()[0].program, OPEN);
    assert_eq!(runner.calls()[0].args, ["https://github.com/acme-org/api"]);
}

#[test]
fn copy_puts_the_url_on_the_clipboard_as_an_argument() {
    let runner = FakeRunner::new().with_success(OSASCRIPT, "");
    let url = "https://github.com/acme-org/api";

    let message = run(
        &runner,
        &Action::Copy {
            url: url.to_owned(),
        },
        true,
    )
    .unwrap();

    assert_eq!(
        message.as_deref(),
        Some("Copied https://github.com/acme-org/api")
    );

    let call = &runner.calls()[0];
    assert_eq!(call.program, OSASCRIPT);
    assert_eq!(call.args[0], "-e");
    assert!(
        !call.args[1].contains(url),
        "the URL must not be interpolated"
    );
    assert_eq!(call.args[2], url);
}

#[test]
fn clone_open_opens_an_existing_clone_without_cloning() {
    let runner = FakeRunner::new().with_success(OPEN, "");
    let action = Action::CloneOpen {
        repo: "acme-org/api".to_owned(),
        editor: "Visual Studio Code".to_owned(),
    };

    let message = run(&runner, &action, true).unwrap();

    assert_eq!(message, None);
    assert_eq!(runner.programs_called(), [OPEN]);
    assert_eq!(
        runner.calls()[0].args,
        [
            "-a",
            "Visual Studio Code",
            "/Users/octo/Workspace/github.com/acme-org/api"
        ]
    );
}

#[test]
fn clone_open_clones_a_missing_repository_first_and_notifies() {
    let runner = FakeRunner::new()
        .with_success(GH, "")
        .with_success(OPEN, "");
    let action = Action::CloneOpen {
        repo: "acme-org/api".to_owned(),
        editor: "Zed".to_owned(),
    };

    let message = run(&runner, &action, false).unwrap();

    assert_eq!(
        message.as_deref(),
        Some("Cloned acme-org/api into /Users/octo/Workspace/github.com/acme-org/api")
    );
    assert_eq!(runner.programs_called(), [GH, OPEN]);
    assert_eq!(
        runner.calls()[0].args,
        [
            "repo",
            "clone",
            "acme-org/api",
            "/Users/octo/Workspace/github.com/acme-org/api"
        ]
    );
}

#[test]
fn clone_open_does_not_open_when_the_clone_fails() {
    let runner = FakeRunner::new()
        .with_failure(GH, "repository not found")
        .with_success(OPEN, "");
    let action = Action::CloneOpen {
        repo: "acme-org/api".to_owned(),
        editor: "Zed".to_owned(),
    };

    assert!(run(&runner, &action, false).is_err());
    assert_eq!(runner.programs_called(), [GH]);
}

#[test]
fn clone_open_rejects_anything_but_owner_slash_repo() {
    for repo in ["--upload-pack=x/y", "acme-org/../etc", "acme-org", "a/b/c"] {
        let runner = FakeRunner::new();
        let action = Action::CloneOpen {
            repo: repo.to_owned(),
            editor: "Zed".to_owned(),
        };

        assert!(run(&runner, &action, false).is_err(), "{repo:?}");
        assert!(runner.calls().is_empty(), "{repo:?} must not run anything");
    }
}

#[test]
fn refresh_clears_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path(), SystemTime::now());
    cache.write(&Key::Viewer, &"octo-user").unwrap();
    let runner = FakeRunner::new();
    let ctx = Context {
        runner: &runner,
        gh: Some(GH),
        home: "/Users/octo".into(),
        cache: &cache,
        exists: &|_: &Path| true,
        jira_url: JIRA_URL,
    };

    let message = action::execute(&ctx, &Action::Refresh).unwrap();

    assert_eq!(message.as_deref(), Some("Cache cleared (1 entry)"));
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[test]
fn a_failing_command_fails_the_action() {
    let runner = FakeRunner::new().with_failure(OPEN, "boom");
    let action = Action::Open {
        url: "https://github.com".to_owned(),
    };

    assert!(run(&runner, &action, true).is_err());
}

#[test]
fn reopen_keeps_the_trailing_space_and_passes_the_query_as_an_argument() {
    assert_eq!(
        Action::parse("reopen gha acme-org/api runs:123 ").unwrap(),
        Action::Reopen {
            query: "gha acme-org/api runs:123 ".to_owned()
        }
    );

    let runner = FakeRunner::new().with_success(OSASCRIPT, "");
    let action = Action::Reopen {
        query: "ghp acme-org/api ".to_owned(),
    };

    assert_eq!(run(&runner, &action, true).unwrap(), None);

    let call = &runner.calls()[0];
    assert_eq!(call.program, OSASCRIPT);
    assert!(call.args[1].contains("com.runningwithcrayons.Alfred"));
    assert!(!call.args[1].contains("acme-org"));
    assert_eq!(call.args[2], "ghp acme-org/api ");
}

#[test]
fn jira_opens_the_ticket_under_jira_url() {
    let runner = FakeRunner::new().with_success(OPEN, "");
    let action = Action::Jira {
        key: Some("PAY-1234".to_owned()),
    };

    assert_eq!(run(&runner, &action, true).unwrap(), None);
    assert_eq!(
        runner.calls()[0].args,
        ["https://jira.example.com/browse/PAY-1234"]
    );
}

#[test]
fn jira_without_a_ticket_or_a_url_only_notifies() {
    let runner = FakeRunner::new();

    let none = run(&runner, &Action::Jira { key: None }, true).unwrap();
    let unset = run_with(
        &runner,
        &Action::Jira {
            key: Some("PAY-1234".to_owned()),
        },
        true,
        "  ",
    )
    .unwrap();

    assert_eq!(none.as_deref(), Some("No Jira ticket in this pull request"));
    assert_eq!(
        unset.as_deref(),
        Some("Set JIRA_URL in the workflow configuration to open Jira tickets")
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn jira_rejects_odd_keys_and_urls() {
    let runner = FakeRunner::new();
    let odd_key = Action::Jira {
        key: Some("../admin".to_owned()),
    };
    let key = Action::Jira {
        key: Some("PAY-1".to_owned()),
    };

    assert!(run(&runner, &odd_key, true).is_err());
    assert!(run_with(&runner, &key, true, "javascript:alert(1)//").is_err());
    assert!(runner.calls().is_empty());
}
