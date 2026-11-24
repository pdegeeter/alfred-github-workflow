//! The single Run Script action every script filter item feeds into.
//!
//! Each item (and each of its modifiers) carries an `arg` of the form
//! `"<verb> <payload>"`. [`Action::parse`] turns it back into an [`Action`] and
//! [`execute`] performs it. Payload fields are ordered so that only the last
//! one may contain spaces; URLs never do.

use anyhow::{anyhow, bail, Result};

use crate::runner::CommandRunner;

/// macOS `open`, used for URLs.
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
}

impl Action {
    /// The AppleScript that puts its first argument on the clipboard.
    const CLIPBOARD_SCRIPT: &'static str =
        "on run argv\nset the clipboard to (item 1 of argv)\nend run";

    /// Encode as the `arg` of an Alfred item.
    pub fn to_arg(&self) -> String {
        match self {
            Action::Open { url } => Action::join("open", &[url]),
            Action::Copy { url } => Action::join("copy", &[url]),
        }
    }

    /// Decode an item `arg` produced by [`Action::to_arg`].
    pub fn parse(arg: &str) -> Result<Action> {
        let arg = arg.trim();
        let (verb, payload) = arg.split_once(' ').unwrap_or((arg, ""));
        let payload = payload.trim();

        let action = match verb {
            "open" => Action::Open {
                url: Action::required(payload, "url")?,
            },
            "copy" => Action::Copy {
                url: Action::required(payload, "url")?,
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
pub fn execute(runner: &dyn CommandRunner, action: &Action) -> Result<Option<String>> {
    match action {
        Action::Open { url } => {
            runner.run(OPEN, &[url])?;

            Ok(None)
        }
        Action::Copy { url } => {
            copy_to_clipboard(runner, url)?;

            Ok(Some(format!("Copied {url}")))
        }
    }
}
