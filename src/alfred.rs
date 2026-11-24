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
use crate::repos::Repo;
use crate::time;

/// Delay before Alfred reruns a script filter whose cache is being refreshed.
pub const RERUN: Duration = Duration::from_millis(500);

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
