//! Tests for the Alfred graph in `workflow/info.plist`: keywords, the
//! subcommand each one runs, the modifier connections to the action and the
//! user configuration.
//!
//! The plist is converted to JSON with macOS `plutil` (CI runs on macOS).

use std::collections::BTreeMap;
use std::process::Command;

use serde_json::Value;

/// Alfred's bitmask for each modifier the script filters emit.
const MODIFIERS: [(u64, &str); 5] = [
    (0, "none"),
    (1_048_576, "cmd"),
    (524_288, "alt"),
    (262_144, "ctrl"),
    (1_179_648, "cmd+shift"),
];

fn plist() -> Value {
    let output = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-", "workflow/info.plist"])
        .output()
        .expect("plutil should run");

    assert!(output.status.success(), "plutil failed: {output:?}");

    serde_json::from_slice(&output.stdout).expect("plist should convert to JSON")
}

fn objects<'a>(plist: &'a Value, kind: &str) -> Vec<&'a Value> {
    plist["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|object| object["type"] == kind)
        .collect()
}

fn script_filters(plist: &Value) -> Vec<&Value> {
    objects(plist, "alfred.workflow.input.scriptfilter")
}

fn action_uid(plist: &Value) -> String {
    let actions = objects(plist, "alfred.workflow.action.script");
    assert_eq!(actions.len(), 1, "exactly one Run Script");
    assert_eq!(actions[0]["config"]["script"], "./alfred-gh run \"$1\"");

    actions[0]["uid"].as_str().unwrap().to_owned()
}

#[test]
fn every_keyword_runs_its_subcommand() {
    let plist = plist();
    let keywords: BTreeMap<String, String> = script_filters(&plist)
        .into_iter()
        .map(|filter| {
            (
                filter["config"]["keyword"].as_str().unwrap().to_owned(),
                filter["config"]["script"].as_str().unwrap().to_owned(),
            )
        })
        .collect();

    let expected: BTreeMap<String, String> = [
        ("gho", "./alfred-gh repos \"$1\""),
        ("gha", "./alfred-gh workflows \"$1\""),
        ("ghm", "./alfred-gh mine \"$1\""),
        ("ghp", "./alfred-gh prs \"$1\""),
        ("ghr", "./alfred-gh review \"$1\""),
    ]
    .into_iter()
    .map(|(keyword, script)| (keyword.to_owned(), script.to_owned()))
    .collect();

    assert_eq!(keywords, expected);
}

#[test]
fn script_filters_let_rust_filter_and_order() {
    let plist = plist();

    for filter in script_filters(&plist) {
        assert_eq!(filter["config"]["alfredfiltersresults"], false);
        assert_eq!(filter["config"]["withspace"], true);
    }
}

#[test]
fn every_script_filter_reaches_the_action_with_every_modifier() {
    let plist = plist();
    let action_uid = action_uid(&plist);

    for filter in script_filters(&plist) {
        let uid = filter["uid"].as_str().unwrap();
        let connections = plist["connections"][uid].as_array().unwrap();

        for (mask, name) in MODIFIERS {
            assert!(
                connections.iter().any(|connection| {
                    connection["destinationuid"] == action_uid.as_str()
                        && connection["modifiers"] == mask
                }),
                "{} has no `{name}` connection to the action",
                filter["config"]["keyword"]
            );
        }
    }
}

#[test]
fn the_action_feeds_a_notification() {
    let plist = plist();
    let action_uid = action_uid(&plist);
    let notification = objects(&plist, "alfred.workflow.output.notification")[0];

    assert_eq!(
        plist["connections"][&action_uid][0]["destinationuid"],
        notification["uid"]
    );
}

#[test]
fn user_configuration_declares_every_variable_without_personal_defaults() {
    let plist = plist();
    let variables: BTreeMap<String, String> = plist["userconfigurationconfig"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| {
            (
                field["variable"].as_str().unwrap().to_owned(),
                field["config"]["default"].as_str().unwrap().to_owned(),
            )
        })
        .collect();

    let expected: BTreeMap<String, String> = [
        ("EDITOR_APP", "Visual Studio Code"),
        ("GH_OWNERS", ""),
        ("JIRA_URL", ""),
    ]
    .into_iter()
    .map(|(variable, default)| (variable.to_owned(), default.to_owned()))
    .collect();

    assert_eq!(variables, expected);
}

#[test]
fn workflow_readme_lists_every_keyword_and_variable() {
    let plist = plist();
    let readme = plist["readme"].as_str().unwrap();

    for filter in script_filters(&plist) {
        let keyword = filter["config"]["keyword"].as_str().unwrap();

        assert!(
            readme.contains(&format!("- {keyword} ")),
            "workflow readme does not document `{keyword}`"
        );
    }

    for field in plist["userconfigurationconfig"].as_array().unwrap() {
        let variable = field["variable"].as_str().unwrap();

        assert!(
            readme.contains(&format!("- {variable} ")),
            "workflow readme does not document `{variable}`"
        );
    }
}
