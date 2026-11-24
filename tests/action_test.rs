//! Tests for the `arg` protocol and the execution of every action.

mod common;

use alfred_gh::action::{self, Action, OPEN, OSASCRIPT};
use common::FakeRunner;

fn every_action() -> Vec<Action> {
    vec![
        Action::Open {
            url: "https://github.com/acme-org/api/pull/42".to_owned(),
        },
        Action::Copy {
            url: "https://github.com/octo-user/dotfiles".to_owned(),
        },
    ]
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

    assert_eq!(verbs, ["open", "copy"]);
}

#[test]
fn parse_rejects_unknown_verbs_and_missing_payloads() {
    assert!(Action::parse("explode https://github.com").is_err());
    assert!(Action::parse("open").is_err());
    assert!(Action::parse("copy   ").is_err());
    assert!(Action::parse("").is_err());
}

#[test]
fn open_hands_the_url_to_macos_open() {
    let runner = FakeRunner::new().with_success(OPEN, "");
    let action = Action::Open {
        url: "https://github.com/acme-org/api".to_owned(),
    };

    let message = action::execute(&runner, &action).unwrap();

    assert_eq!(message, None);
    assert_eq!(runner.calls()[0].program, OPEN);
    assert_eq!(runner.calls()[0].args, ["https://github.com/acme-org/api"]);
}

#[test]
fn copy_puts_the_url_on_the_clipboard_as_an_argument() {
    let runner = FakeRunner::new().with_success(OSASCRIPT, "");
    let url = "https://github.com/acme-org/api";

    let message = action::execute(
        &runner,
        &Action::Copy {
            url: url.to_owned(),
        },
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
fn a_failing_command_fails_the_action() {
    let runner = FakeRunner::new().with_failure(OPEN, "boom");
    let action = Action::Open {
        url: "https://github.com".to_owned(),
    };

    assert!(action::execute(&runner, &action).is_err());
}
