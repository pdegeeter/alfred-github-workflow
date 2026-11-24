//! The single Run Script action every script filter item feeds into.
//!
//! Each item (and each of its modifiers) carries an `arg` of the form
//! `"<verb> <payload>"`. [`Action::parse`] turns it back into an [`Action`] and
//! [`execute`] performs it. Payload fields are ordered so that only the last
//! one may contain spaces (application names); URLs and `owner/repo` never do.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};

use crate::cache::Cache;
use crate::jira;
use crate::local;
use crate::runner::CommandRunner;

/// macOS `open`, used for URLs and to open clones in an editor.
pub const OPEN: &str = "/usr/bin/open";

/// `osascript`, used for the clipboard.
pub const OSASCRIPT: &str = "/usr/bin/osascript";

/// Something the user asked for by actioning an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open a URL in the default browser.
    Open { url: String },
    /// Copy a URL.
    Copy { url: String },
    /// Open the local clone of `repo` in `editor`, cloning it first if needed.
    CloneOpen { repo: String, editor: String },
    /// Clear every cache entry.
    Refresh,
    /// Reopen Alfred with `query` typed in (drill down to another keyword).
    Reopen { query: String },
    /// Open a Jira ticket; `None` when the pull request names none.
    Jira { key: Option<String> },
}

/// What [`execute`] needs from the outside world.
pub struct Context<'a> {
    pub runner: &'a dyn CommandRunner,
    /// The resolved `gh` binary, if installed.
    pub gh: Option<&'a str>,
    pub home: PathBuf,
    pub cache: &'a Cache,
    /// Whether a path exists (injected so clone checks are testable).
    pub exists: &'a dyn Fn(&Path) -> bool,
    /// `JIRA_URL`, empty when unset.
    pub jira_url: &'a str,
}

impl Action {
    /// The AppleScript that puts its first argument on the clipboard.
    const CLIPBOARD_SCRIPT: &'static str =
        "on run argv\nset the clipboard to (item 1 of argv)\nend run";

    /// The AppleScript that shows Alfred (5 or later) with its first argument
    /// typed in.
    const ALFRED_SEARCH_SCRIPT: &'static str = "on run argv\ntell application id \"com.runningwithcrayons.Alfred\" to search (item 1 of argv)\nend run";

    /// Encode as the `arg` of an Alfred item.
    pub fn to_arg(&self) -> String {
        match self {
            Action::Open { url } => Action::join("open", &[url]),
            Action::Copy { url } => Action::join("copy", &[url]),
            Action::CloneOpen { repo, editor } => Action::join("clone-open", &[repo, editor]),
            Action::Refresh => Action::join("refresh", &["all"]),
            Action::Reopen { query } => format!("reopen {query}"),
            Action::Jira { key } => Action::join("jira", &[key.as_deref().unwrap_or("")]),
        }
    }

    /// Decode an item `arg` produced by [`Action::to_arg`].
    pub fn parse(raw: &str) -> Result<Action> {
        let arg = raw.trim();
        let (verb, payload) = arg.split_once(' ').unwrap_or((arg, ""));
        let payload = payload.trim();

        let action = match verb {
            "open" => Action::Open {
                url: Action::required(payload, "url")?,
            },
            "copy" => Action::Copy {
                url: Action::required(payload, "url")?,
            },
            "clone-open" => {
                let (repo, editor) = payload
                    .split_once(' ')
                    .ok_or_else(|| anyhow!("missing editor in {arg:?}"))?;

                Action::CloneOpen {
                    repo: Action::required(repo, "repository")?,
                    editor: Action::required(editor.trim(), "editor")?,
                }
            }
            "refresh" if payload == "all" => Action::Refresh,
            "reopen" => Action::Reopen {
                // Keep the trailing space of `ghp owner/repo ` so Alfred
                // shows the next level at once.
                query: Action::required(
                    raw.trim_start()
                        .strip_prefix("reopen ")
                        .unwrap_or_default()
                        .trim_start(),
                    "query",
                )?,
            },
            "jira" => Action::Jira {
                key: (!payload.is_empty()).then(|| payload.to_owned()),
            },
            other => bail!("unknown action: {other:?}"),
        };

        Ok(action)
    }

    fn join(verb: &str, fields: &[&str]) -> String {
        let mut arg = verb.to_owned();

        for field in fields {
            if !field.is_empty() {
                arg.push(' ');
                arg.push_str(field);
            }
        }

        arg
    }

    fn required(value: &str, what: &str) -> Result<String> {
        if value.is_empty() {
            return Err(anyhow!("missing {what}"));
        }

        Ok(value.to_owned())
    }
}

/// Put `text` on the clipboard.
pub fn copy_to_clipboard(runner: &dyn CommandRunner, text: &str) -> Result<()> {
    runner
        .run(OSASCRIPT, &["-e", Action::CLIPBOARD_SCRIPT, text])
        .map(|_| ())
}

/// Perform `action`. Returns the text of the notification to show, if any.
pub fn execute(ctx: &Context, action: &Action) -> Result<Option<String>> {
    match action {
        Action::Open { url } => {
            ctx.runner.run(OPEN, &[url])?;

            Ok(None)
        }
        Action::Copy { url } => {
            copy_to_clipboard(ctx.runner, url)?;

            Ok(Some(format!("Copied {url}")))
        }
        Action::CloneOpen { repo, editor } => clone_open(ctx, repo, editor),
        Action::Refresh => {
            let removed = ctx.cache.clear()?;
            let plural = if removed == 1 { "y" } else { "ies" };

            Ok(Some(format!("Cache cleared ({removed} entr{plural})")))
        }
        Action::Reopen { query } => {
            ctx.runner
                .run(OSASCRIPT, &["-e", Action::ALFRED_SEARCH_SCRIPT, query])?;

            Ok(None)
        }
        Action::Jira { key } => open_jira(ctx, key.as_deref()),
    }
}

fn open_jira(ctx: &Context, key: Option<&str>) -> Result<Option<String>> {
    let Some(key) = key else {
        return Ok(Some("No Jira ticket in this pull request".to_owned()));
    };

    let base = ctx.jira_url.trim();

    if base.is_empty() {
        return Ok(Some(
            "Set JIRA_URL in the workflow configuration to open Jira tickets".to_owned(),
        ));
    }

    if !base.starts_with("https://") && !base.starts_with("http://") {
        bail!("JIRA_URL must start with https://");
    }

    if jira::find_key(key).as_deref() != Some(key) {
        bail!("invalid Jira key: {key:?}");
    }

    ctx.runner.run(OPEN, &[&format!("{base}{key}")])?;

    Ok(None)
}

fn clone_open(ctx: &Context, repo: &str, editor: &str) -> Result<Option<String>> {
    if !local::is_valid_name_with_owner(repo) {
        bail!("invalid repository: {repo:?}");
    }

    let path = local::clone_path(&ctx.home, repo);
    let path_arg = path.to_string_lossy().into_owned();
    let mut message = None;

    if !(ctx.exists)(&path) {
        let gh = ctx
            .gh
            .ok_or_else(|| anyhow!("GitHub CLI not found: brew install gh"))?;

        ctx.runner.run(gh, &["repo", "clone", repo, &path_arg])?;
        message = Some(format!("Cloned {repo} into {path_arg}"));
    }

    ctx.runner.run(OPEN, &["-a", editor, &path_arg])?;

    Ok(message)
}
