//! Script filter items and output.
//!
//! Every actionable item (and each of its modifiers) carries an [`Action`]
//! encoded in its `arg`; the one connected Run Script decodes and performs it.
//! No `uid` is set: Alfred would otherwise re-sort items by its own usage
//! knowledge and override the order computed here.

use std::time::Duration;

use powerpack::{Item, Key, Modifier, Output};

use crate::action::Action;
use crate::gh::{GhError, SSO_SETTINGS_URL};
use crate::jira;
use crate::prs::Pr;
use crate::repos::Repo;
use crate::time;
use crate::workflows::{Run, Workflow};

/// Delay before Alfred reruns a script filter whose cache is being refreshed.
pub const RERUN: Duration = Duration::from_millis(500);

/// Keyword of the pull requests script filter, reopened by drill-downs.
pub const PRS_KEYWORD: &str = "ghp";

/// Keyword of the Actions script filter, reopened by drill-downs.
pub const WORKFLOWS_KEYWORD: &str = "gha";

/// Item shown in place of a list when something went wrong.
pub fn error_item(title: &str, message: &str) -> Item {
    Item::new(title).subtitle(message).valid(false)
}

/// The item explaining a `gh` failure and its fix. `owner` names the owner
/// whose fetch failed, when the failure is specific to one.
pub fn gh_error_item(owner: Option<&str>, error: &GhError, now: i64) -> Item {
    let prefix = owner.map(|owner| format!("{owner}: ")).unwrap_or_default();

    match error {
        GhError::Missing => error_item(
            "GitHub CLI not found",
            "Install it with: brew install gh, then run: gh auth login",
        ),
        GhError::NotAuthenticated => {
            error_item("GitHub CLI is not authenticated", "Run: gh auth login")
        }
        GhError::RateLimited { reset } => {
            let when = match reset {
                Some(reset) => format!("Resets in {}", time::duration((reset - now).max(0))),
                None => "Try again later".to_owned(),
            };

            error_item(&format!("{prefix}GitHub API rate limit reached"), &when)
        }
        GhError::SsoRequired => sso_item(owner.unwrap_or("GitHub")),
        GhError::Other(message) => error_item(&format!("{prefix}GitHub error"), message),
    }
}

/// The item sending the user to authorize `gh` for a SAML organization.
pub fn sso_item(owner: &str) -> Item {
    Item::new(format!("{owner}: token not authorized for SSO"))
        .subtitle("↩ authorize the GitHub CLI for this organization")
        .arg(
            Action::Open {
                url: SSO_SETTINGS_URL.to_owned(),
            }
            .to_arg(),
        )
}

fn modifier(key: Key, subtitle: &str, action: &Action) -> Modifier {
    Modifier::new(key).subtitle(subtitle).arg(action.to_arg())
}

fn subtitle(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A repository: ↩ open on GitHub, ⌘ open the local clone, ⌥ copy the URL.
pub fn repo_item(repo: &Repo, editor: &str, now: i64) -> Item {
    let mut flags = Vec::new();

    if repo.is_archived {
        flags.push("Archived");
    }

    if repo.is_private {
        flags.push("🔒");
    }

    if repo.is_fork {
        flags.push("Fork");
    }

    let pushed = repo
        .pushed_at
        .as_deref()
        .map(|pushed| time::ago_iso(pushed, now))
        .filter(|ago| !ago.is_empty())
        .map(|ago| format!("pushed {ago}"))
        .unwrap_or_default();

    let subtitle = subtitle(vec![
        flags.join(" "),
        repo.language.clone().unwrap_or_default(),
        repo.description.clone().unwrap_or_default(),
        pushed,
    ]);

    Item::new(repo.name_with_owner.clone())
        .subtitle(subtitle)
        .arg(
            Action::Open {
                url: repo.url.clone(),
            }
            .to_arg(),
        )
        .copy_text(repo.url.clone())
        .modifier(modifier(
            Key::Command,
            &format!("Open the local clone in {editor} (clones it if needed)"),
            &Action::CloneOpen {
                repo: repo.name_with_owner.clone(),
                editor: editor.to_owned(),
            },
        ))
        .modifier(modifier(
            Key::Option,
            "Copy the URL",
            &Action::Copy {
                url: repo.url.clone(),
            },
        ))
        .modifier(modifier(
            Key::Control,
            "Pull requests",
            &Action::Reopen {
                query: format!("{PRS_KEYWORD} {} ", repo.name_with_owner),
            },
        ))
        .modifier(
            Modifier::new_multi([Key::Command, Key::Shift])
                .subtitle("Actions workflows")
                .arg(
                    Action::Reopen {
                        query: format!("{WORKFLOWS_KEYWORD} {} ", repo.name_with_owner),
                    }
                    .to_arg(),
                ),
        )
}

fn run_details(run: &Run, now: i64) -> Vec<String> {
    vec![
        run.branch.clone(),
        run.event.clone(),
        run.actor.clone().unwrap_or_default(),
        run.duration(now).map(time::duration).unwrap_or_default(),
        time::ago_iso(&run.started_at, now),
    ]
}

/// A workflow with the status of its last run on the default branch:
/// ↩ its runs page, ⌘ the last run, ⌥ copy the runs page URL, ⌃ list its
/// runs in Alfred.
pub fn workflow_item(repo: &str, workflow: &Workflow, last: Option<&Run>, now: i64) -> Item {
    let runs_url = workflow.runs_url(repo);

    let mut details = vec![workflow.path.clone()];

    let status = if workflow.is_disabled() {
        "⏸ disabled".to_owned()
    } else {
        match last {
            Some(run) => {
                details.extend(run_details(run, now));
                run.status().icon().to_owned()
            }
            None => "no recent run".to_owned(),
        }
    };

    details.insert(0, status);

    let (last_url, last_subtitle) = match last {
        Some(run) => (run.url.clone(), format!("Open run #{}", run.run_number)),
        None => (
            runs_url.clone(),
            "No recent run: open the runs page".to_owned(),
        ),
    };

    Item::new(workflow.name.clone())
        .subtitle(subtitle(details))
        .arg(
            Action::Open {
                url: runs_url.clone(),
            }
            .to_arg(),
        )
        .copy_text(runs_url.clone())
        .modifier(modifier(
            Key::Command,
            &last_subtitle,
            &Action::Open { url: last_url },
        ))
        .modifier(modifier(
            Key::Option,
            "Copy the URL",
            &Action::Copy { url: runs_url },
        ))
        .modifier(modifier(
            Key::Control,
            "List its runs",
            &Action::Reopen {
                query: format!("{WORKFLOWS_KEYWORD} {repo} runs:{} ", workflow.id),
            },
        ))
}

/// A workflow run: ↩ open, ⌥ copy the URL.
pub fn run_item(run: &Run, now: i64) -> Item {
    Item::new(format!(
        "{} #{} · {}",
        run.status().icon(),
        run.run_number,
        run.title
    ))
    .subtitle(subtitle(run_details(run, now)))
    .arg(
        Action::Open {
            url: run.url.clone(),
        }
        .to_arg(),
    )
    .copy_text(run.url.clone())
    .modifier(modifier(
        Key::Option,
        "Copy the URL",
        &Action::Copy {
            url: run.url.clone(),
        },
    ))
}

/// A repository offered while resolving `ghp` / `gha`: ↩ (or ⇥) completes
/// the query with `owner/name `.
pub fn candidate_item(repo: &Repo, now: i64) -> Item {
    let pushed = repo
        .pushed_at
        .as_deref()
        .map(|pushed| time::ago_iso(pushed, now))
        .filter(|ago| !ago.is_empty())
        .map(|ago| format!("pushed {ago}"))
        .unwrap_or_default();

    Item::new(repo.name_with_owner.clone())
        .subtitle(subtitle(vec![
            "↩ choose this repository".to_owned(),
            repo.description.clone().unwrap_or_default(),
            pushed,
        ]))
        .autocomplete(format!("{} ", repo.name_with_owner))
        .valid(false)
}

/// A pull request: ↩ open, ⌘ "Files changed", ⌥ copy the URL, ⌃ the Jira
/// ticket named in its title or branch. `show_repo` prefixes the title with
/// the repository (lists across repositories).
pub fn pr_item(pr: &Pr, show_repo: bool, now: i64) -> Item {
    let title = if show_repo {
        format!("{} · {}", pr.repo, pr.title)
    } else {
        pr.title.clone()
    };

    let state = match pr.state.as_str() {
        "MERGED" => "merged",
        "CLOSED" => "closed",
        _ if pr.is_draft => "draft",
        _ => "",
    };

    let updated = time::ago_iso(&pr.updated_at, now);

    let subtitle = subtitle(vec![
        format!("#{}", pr.number),
        pr.author.clone().unwrap_or_else(|| "ghost".to_owned()),
        format!("{}→{}", pr.head, pr.base),
        state.to_owned(),
        pr.checks
            .map(|checks| format!("CI {}", checks.icon()))
            .unwrap_or_default(),
        pr.review_label().unwrap_or_default().to_owned(),
        if updated.is_empty() {
            String::new()
        } else {
            format!("updated {updated}")
        },
    ]);

    let ticket = jira::pr_key(&pr.title, &pr.head);
    let jira_subtitle = match &ticket {
        Some(key) => format!("Open {key} in Jira"),
        None => "No Jira ticket in the title or branch".to_owned(),
    };

    Item::new(title)
        .subtitle(subtitle)
        .arg(
            Action::Open {
                url: pr.url.clone(),
            }
            .to_arg(),
        )
        .copy_text(pr.url.clone())
        .modifier(modifier(
            Key::Command,
            "Open “Files changed”",
            &Action::Open {
                url: pr.files_url(),
            },
        ))
        .modifier(modifier(
            Key::Option,
            "Copy the URL",
            &Action::Copy {
                url: pr.url.clone(),
            },
        ))
        .modifier(modifier(
            Key::Control,
            &jira_subtitle,
            &Action::Jira { key: ticket },
        ))
}

/// The item clearing every cache entry.
pub fn refresh_item() -> Item {
    Item::new("Refresh cache")
        .subtitle("Forget cached repositories, pull requests and runs")
        .arg(Action::Refresh.to_arg())
}

/// The script filter output for `items`. With `rerun`, Alfred runs the script
/// filter again after that delay: used while a stale cache is refreshed in the
/// background, so the fresh results replace the stale ones.
pub fn output(items: Vec<Item>, rerun: Option<Duration>) -> Output {
    let mut output = Output::new();
    output.items(items);

    if let Some(rerun) = rerun {
        output.rerun(rerun);
    }

    output
}
