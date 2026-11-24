//! Stale-while-revalidate JSON cache under `$alfred_workflow_cache`.
//!
//! Each entry is a file `<key>.json` holding the value and the Unix time it
//! was fetched at. Freshness is decided from that timestamp and an injected
//! clock (`now`), never from file metadata, so the logic is deterministic.
//!
//! * fresh — served as is;
//! * stale — served immediately while a detached `alfred-gh refresh <key>`
//!   rewrites it, and Alfred is asked to rerun the script filter;
//! * missing or unreadable — fetched synchronously.
//!
//! A refresh holds `<key>.lock` (containing its start time) so concurrent
//! script filter runs do not start a second one. A lock older than
//! [`Cache::LOCK_TTL`] is ignored: that is also how a failed refresh backs off
//! (it leaves its lock behind instead of being retried on every rerun).

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::prs::PrState;
use crate::time;

/// What a cache entry holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// The authenticated user's login (default owner).
    Viewer,
    /// The repositories of one owner.
    Repos { owner: String },
    /// The pull requests of one repository (`owner/name`) in one state.
    Prs { repo: String, state: PrState },
    /// The Actions workflows of one repository and its default branch.
    Workflows { repo: String },
    /// The latest runs of one repository on one branch (last run per workflow).
    BranchRuns { repo: String, branch: String },
    /// The latest runs of one workflow.
    Runs { repo: String, workflow_id: u64 },
}

impl Key {
    /// File name stem, safe for any owner or repository name.
    pub fn stem(&self) -> String {
        let raw = match self {
            Key::Viewer => "viewer".to_owned(),
            Key::Repos { owner } => format!("repos-{owner}"),
            Key::Prs { repo, state } => format!("prs-{repo}-{}", state.as_str()),
            Key::Workflows { repo } => format!("workflows-{repo}"),
            Key::BranchRuns { repo, branch } => format!("branch-runs-{repo}-{branch}"),
            Key::Runs { repo, workflow_id } => format!("runs-{repo}-{workflow_id}"),
        };

        raw.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// How long an entry stays fresh.
    pub fn ttl(&self) -> Duration {
        match self {
            Key::Viewer | Key::Repos { .. } => Duration::from_secs(3_600),
            Key::Prs { .. } => Duration::from_secs(60),
            Key::Workflows { .. } => Duration::from_secs(600),
            Key::BranchRuns { .. } | Key::Runs { .. } => Duration::from_secs(30),
        }
    }

    /// Encode as the argument of `alfred-gh refresh` (only the last field may
    /// contain spaces).
    pub fn to_arg(&self) -> String {
        match self {
            Key::Viewer => "viewer".to_owned(),
            Key::Repos { owner } => format!("repos {owner}"),
            Key::Prs { repo, state } => format!("prs {repo} {}", state.as_str()),
            Key::Workflows { repo } => format!("workflows {repo}"),
            Key::BranchRuns { repo, branch } => format!("branch-runs {repo} {branch}"),
            Key::Runs { repo, workflow_id } => format!("runs {repo} {workflow_id}"),
        }
    }

    /// Decode an argument produced by [`Key::to_arg`].
    pub fn parse(arg: &str) -> Result<Key> {
        let fields: Vec<&str> = arg.split_whitespace().collect();

        let key = match fields.as_slice() {
            ["viewer"] => Key::Viewer,
            ["repos", owner] => Key::Repos {
                owner: (*owner).to_owned(),
            },
            ["prs", repo, state] => Key::Prs {
                repo: (*repo).to_owned(),
                state: PrState::parse(state)
                    .ok_or_else(|| anyhow!("invalid pull request state: {state:?}"))?,
            },
            ["workflows", repo] => Key::Workflows {
                repo: (*repo).to_owned(),
            },
            ["branch-runs", repo, branch] => Key::BranchRuns {
                repo: (*repo).to_owned(),
                branch: (*branch).to_owned(),
            },
            ["runs", repo, id] => Key::Runs {
                repo: (*repo).to_owned(),
                workflow_id: id
                    .parse()
                    .map_err(|_| anyhow!("invalid workflow id: {id:?}"))?,
            },
            _ => bail!("invalid cache key: {arg:?}"),
        };

        Ok(key)
    }
}

/// The state of a cache entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry<T> {
    Fresh(T),
    Stale(T),
    Missing,
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    fetched_at: i64,
    value: T,
}

/// The cache directory and the current time.
pub struct Cache {
    dir: PathBuf,
    now: SystemTime,
}

impl Cache {
    /// How long a refresh lock is honoured.
    pub const LOCK_TTL: Duration = Duration::from_secs(60);

    pub fn new(dir: impl Into<PathBuf>, now: SystemTime) -> Self {
        Self {
            dir: dir.into(),
            now,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn now(&self) -> SystemTime {
        self.now
    }

    fn path(&self, key: &Key, extension: &str) -> PathBuf {
        self.dir.join(format!("{}.{extension}", key.stem()))
    }

    /// Read an entry. A corrupted or unreadable file counts as missing.
    pub fn read<T: DeserializeOwned>(&self, key: &Key) -> Entry<T> {
        let Ok(bytes) = fs::read(self.path(key, "json")) else {
            return Entry::Missing;
        };

        let Ok(envelope) = serde_json::from_slice::<Envelope<T>>(&bytes) else {
            return Entry::Missing;
        };

        let age = time::unix(self.now) - envelope.fetched_at;

        if age >= 0 && age < key.ttl().as_secs() as i64 {
            return Entry::Fresh(envelope.value);
        }

        Entry::Stale(envelope.value)
    }

    /// Write an entry atomically (temporary file, then rename).
    pub fn write<T: Serialize>(&self, key: &Key, value: &T) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("cannot create {}", self.dir.display()))?;

        let envelope = Envelope {
            fetched_at: time::unix(self.now),
            value,
        };
        let path = self.path(key, "json");
        let tmp = self.path(key, &format!("json.{}.tmp", std::process::id()));

        fs::write(&tmp, serde_json::to_vec(&envelope)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))?;

        Ok(())
    }

    /// Whether a refresh of `key` is in progress (a lock younger than
    /// [`Cache::LOCK_TTL`]).
    pub fn is_locked(&self, key: &Key) -> bool {
        let Ok(content) = fs::read_to_string(self.path(key, "lock")) else {
            return false;
        };

        let Ok(started) = content.trim().parse::<i64>() else {
            return false;
        };

        time::unix(self.now) - started < Cache::LOCK_TTL.as_secs() as i64
    }

    /// Take the refresh lock of `key`, unless a live one exists. An expired
    /// lock is replaced.
    pub fn try_lock(&self, key: &Key) -> Result<Option<Lock>> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("cannot create {}", self.dir.display()))?;

        let path = self.path(key, "lock");

        if !self.is_locked(key) {
            let _ = fs::remove_file(&path);
        }

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                write!(file, "{}", time::unix(self.now))?;

                Ok(Some(Lock { path }))
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(None),
            Err(err) => Err(err).with_context(|| format!("cannot create {}", path.display())),
        }
    }

    /// Remove every entry and lock.
    pub fn clear(&self) -> Result<usize> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(0);
        };

        let mut removed = 0;

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();

            if name.ends_with(".json") || name.ends_with(".lock") {
                fs::remove_file(&path)
                    .with_context(|| format!("cannot remove {}", path.display()))?;
                removed += 1;
            }
        }

        Ok(removed)
    }
}

/// A held refresh lock. [`Lock::release`] removes it; dropping it without
/// releasing leaves it in place until it expires (used after a failure).
#[derive(Debug)]
pub struct Lock {
    path: PathBuf,
}

impl Lock {
    pub fn release(self) {
        let _ = fs::remove_file(&self.path);
    }
}
