//! Tests for Actions parsing and the `gha` script filter (workflows with
//! their last run, runs of one workflow).

mod common;

use alfred_gh::cache::Key;
use alfred_gh::gh::Gh;
use alfred_gh::workflows::{self, Run, RunStatus, Workflow};
use common::{app, items_json, titles, FakeRunner, GH, NOW};
use serde_json::Value;

const WORKFLOWS: &str = include_str!("fixtures/actions_workflows.json");
const BRANCH_RUNS: &str = include_str!("fixtures/actions_runs_branch.json");
const WORKFLOW_RUNS: &str = include_str!("fixtures/actions_runs_workflow.json");
const ORG: &str = include_str!("fixtures/repos_org.json");

fn body(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn branch_runs() -> Vec<Run> {
    workflows::parse_runs(&body(BRANCH_RUNS))
}

#[test]
fn workflows_parse_with_their_state_and_runs_page() {
    let parsed = workflows::parse_workflows(&body(WORKFLOWS));

    assert_eq!(parsed.len(), 3);
    assert_eq!(
        parsed[0],
        Workflow {
            id: 1001,
            name: "CI".to_owned(),
            path: ".github/workflows/ci.yml".to_owned(),
            state: "active".to_owned(),
        }
    );
    assert!(parsed[2].is_disabled());
    assert_eq!(
        parsed[0].runs_url("acme-org/api"),
        "https://github.com/acme-org/api/actions/workflows/ci.yml"
    );
}

#[test]
fn runs_parse_and_the_newest_per_workflow_is_its_last_run() {
    let runs = branch_runs();

    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].title, "Fix rounding (#42)");
    assert_eq!(runs[0].actor.as_deref(), Some("octo-user"));

    assert_eq!(workflows::last_run(&runs, 1001).unwrap().id, 9003);
    assert_eq!(workflows::last_run(&runs, 1002).unwrap().id, 9002);
    assert_eq!(workflows::last_run(&runs, 1003), None);
}

#[test]
fn run_status_and_duration() {
    let runs = branch_runs();
    let in_progress = &runs[0];
    let failure = &runs[1];
    let success = &runs[2];

    assert_eq!(in_progress.status(), RunStatus::InProgress);
    assert_eq!(failure.status(), RunStatus::Failure);
    assert_eq!(success.status(), RunStatus::Success);
    assert_eq!(success.duration(0), Some(192));
    assert_eq!(
        in_progress.duration(1_795_564_800),
        Some(120),
        "runs until now"
    );

    let mut cancelled = success.clone();
    cancelled.conclusion = Some("cancelled".to_owned());
    assert_eq!(cancelled.status(), RunStatus::Other);

    let icons: Vec<&str> = [
        RunStatus::Success,
        RunStatus::Failure,
        RunStatus::InProgress,
        RunStatus::Other,
    ]
    .iter()
    .map(|status| status.icon())
    .collect();
    assert_eq!(icons, ["✅", "❌", "🟡", "⚪"]);
}

#[test]
fn branch_names_are_percent_encoded() {
    assert_eq!(workflows::encode("main"), "main");
    assert_eq!(workflows::encode("release/1.0"), "release%2F1.0");
    assert_eq!(workflows::encode("a&b=c#d e"), "a%26b%3Dc%23d%20e");
}

#[test]
fn fetches_use_three_fixed_rest_paths() {
    let runner = FakeRunner::new()
        .with_success(GH, r#"{"default_branch":"develop"}"#)
        .with_success(GH, WORKFLOWS)
        .with_success(GH, BRANCH_RUNS)
        .with_success(GH, WORKFLOW_RUNS);
    let gh = Gh::new(&runner, Some(GH));

    let list = workflows::fetch_workflows(&gh, "acme-org/api", None).unwrap();
    workflows::fetch_branch_runs(&gh, "acme-org/api", "release/1.0").unwrap();
    workflows::fetch_workflow_runs(&gh, "acme-org/api", 1001).unwrap();

    assert_eq!(list.default_branch, "develop");

    let paths: Vec<String> = runner
        .calls()
        .iter()
        .map(|call| call.args.join(" "))
        .collect();
    assert_eq!(
        paths,
        [
            "api repos/acme-org/api",
            "api repos/acme-org/api/actions/workflows?per_page=100",
            "api repos/acme-org/api/actions/runs?branch=release%2F1.0&per_page=100",
            "api repos/acme-org/api/actions/workflows/1001/runs?per_page=10",
        ]
    );
}

#[test]
fn a_known_default_branch_skips_the_repository_call() {
    let runner = FakeRunner::new().with_success(GH, WORKFLOWS);

    workflows::fetch_workflows(&Gh::new(&runner, Some(GH)), "acme-org/api", Some("main")).unwrap();

    assert_eq!(runner.calls().len(), 1);
}

#[test]
fn an_invalid_repository_never_reaches_gh() {
    let runner = FakeRunner::new();
    let gh = Gh::new(&runner, Some(GH));

    assert!(workflows::fetch_workflows(&gh, "acme-org/../x", Some("main")).is_err());
    assert!(workflows::fetch_workflow_runs(&gh, "-x/y", 1).is_err());
    assert!(runner.calls().is_empty());
}

#[test]
fn gha_lists_workflows_with_their_last_run_on_the_cached_default_branch() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, ORG)
        .with_success(GH, WORKFLOWS)
        .with_success(GH, BRANCH_RUNS);
    let app = app(&runner, dir.path(), NOW, "acme-org");
    app.repos("");

    let items = items_json(&app.workflows("acme-org/api ").items);

    assert_eq!(
        runner.calls().len(),
        3,
        "no repository call: the default branch is cached"
    );
    assert_eq!(
        runner.calls()[2].args[1],
        "repos/acme-org/api/actions/runs?branch=main&per_page=100"
    );
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["title"], "CI");
    assert!(items[0]["subtitle"]
        .as_str()
        .unwrap()
        .starts_with("🟡 · .github/workflows/ci.yml · main · push · octo-user"));
    assert!(items[1]["subtitle"].as_str().unwrap().starts_with("❌ · "));
    assert_eq!(
        items[2]["subtitle"],
        "⏸ disabled · .github/workflows/nightly.yml"
    );
    assert_eq!(
        items[0]["mods"]["cmd"]["arg"],
        "open https://github.com/acme-org/api/actions/runs/9003"
    );
    assert_eq!(
        items[0]["mods"]["ctrl"]["arg"],
        "reopen gha acme-org/api runs:1001 "
    );
}

#[test]
fn gha_filters_workflows_by_name_or_path() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new()
        .with_success(GH, r#"{"default_branch":"main"}"#)
        .with_success(GH, WORKFLOWS)
        .with_success(GH, BRANCH_RUNS);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    assert_eq!(
        titles(&app.workflows("acme-org/api release").items),
        ["release-please"]
    );
    assert_eq!(
        titles(&app.workflows("acme-org/api nightly.yml").items),
        ["Nightly"]
    );

    let items = items_json(&app.workflows("acme-org/api zzz").items);
    assert_eq!(items[0]["title"], "No matching workflow in acme-org/api");
}

#[test]
fn gha_runs_lists_the_runs_of_one_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_success(GH, WORKFLOW_RUNS);
    let app = app(&runner, dir.path(), NOW, "acme-org");

    let items = items_json(&app.workflows("acme-org/api runs:1001").items);

    assert_eq!(runner.calls().len(), 1);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "⚪ #32 · Add retries");
    assert_eq!(
        items[0]["subtitle"],
        "feat/retries · pull_request · dev-two · 30s · 2h ago"
    );
    assert_eq!(
        items[0]["arg"],
        "open https://github.com/acme-org/api/actions/runs/9004"
    );
    assert_eq!(
        items[0]["mods"]["alt"]["arg"],
        "copy https://github.com/acme-org/api/actions/runs/9004"
    );
    assert_eq!(
        titles(&app.workflows("acme-org/api runs:1001 main").items),
        ["✅ #30 · Bump deps (#40)"]
    );
}

#[test]
fn gha_serves_stale_runs_while_refreshing() {
    let dir = tempfile::tempdir().unwrap();
    let warm = FakeRunner::new().with_success(GH, WORKFLOW_RUNS);
    app(&warm, dir.path(), NOW, "acme-org").workflows("acme-org/api runs:1001");

    let runner = FakeRunner::new();
    let listing =
        app(&runner, dir.path(), NOW + 30, "acme-org").workflows("acme-org/api runs:1001");

    assert!(listing.rerun);
    assert_eq!(
        runner.spawns()[0].args,
        ["refresh", "runs acme-org/api 1001"]
    );
}

#[test]
fn gha_reports_errors() {
    let dir = tempfile::tempdir().unwrap();
    let runner = FakeRunner::new().with_failure(GH, "gh: Not Found (HTTP 404)");

    let items = items_json(
        &app(&runner, dir.path(), NOW, "acme-org")
            .workflows("acme-org/nope ")
            .items,
    );

    assert_eq!(items[0]["title"], "acme-org/nope: GitHub error");
    assert_eq!(items[0]["subtitle"], "Not Found (HTTP 404)");
}

#[test]
fn every_actions_key_round_trips() {
    for key in [
        Key::Workflows {
            repo: "acme-org/api".to_owned(),
        },
        Key::BranchRuns {
            repo: "acme-org/api".to_owned(),
            branch: "release/1.0".to_owned(),
        },
        Key::Runs {
            repo: "acme-org/api".to_owned(),
            workflow_id: 1001,
        },
    ] {
        assert_eq!(Key::parse(&key.to_arg()).unwrap(), key);
    }

    assert_eq!(
        Key::Workflows {
            repo: "a/b".to_owned()
        }
        .ttl()
        .as_secs(),
        600
    );
    assert_eq!(
        Key::Runs {
            repo: "a/b".to_owned(),
            workflow_id: 1
        }
        .ttl()
        .as_secs(),
        30
    );
    assert!(Key::parse("runs acme-org/api nope").is_err());
}
