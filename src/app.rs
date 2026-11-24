//! Script filter orchestration: settings, cache loading and the items of each
//! keyword. It only touches the world through [`CommandRunner`] and the cache
//! directory, so it is tested with `FakeRunner` and a temporary directory.

use std::path::PathBuf;

use anyhow::Result;
use powerpack::Item;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::alfred;
use crate::cache::{Cache, Entry, Key};
use crate::gh::{Gh, GhError};
use crate::local;
use crate::owners::{self, OwnerKind};
use crate::prs::{self, Search};
use crate::query;
use crate::ranking::{self, Resolution};
use crate::repos::{self, OwnerRepos, Repo};
use crate::runner::CommandRunner;
use crate::time;
use crate::workflows;

/// Workflow variables (Alfred's user configuration).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `GH_OWNERS`, parsed. Empty means the authenticated user.
    pub owners: Vec<String>,
    /// `EDITOR_APP`.
    pub editor_app: String,
    /// `JIRA_URL`, empty when unset.
    pub jira_url: String,
}

impl Settings {
    pub const DEFAULT_EDITOR: &'static str = "Visual Studio Code";

    /// Read the settings through `var` (the process environment in prod).
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Settings {
        let value = |name: &str| var(name).map(|value| value.trim().to_owned());

        Settings {
            owners: owners::parse(&value("GH_OWNERS").unwrap_or_default()),
            editor_app: value("EDITOR_APP")
                .filter(|editor| !editor.is_empty())
                .unwrap_or_else(|| Settings::DEFAULT_EDITOR.to_owned()),
            jira_url: value("JIRA_URL").unwrap_or_default(),
        }
    }
}

/// What a script filter prints.
pub struct Listing {
    pub items: Vec<Item>,
    /// A stale entry is being refreshed: ask Alfred to rerun.
    pub rerun: bool,
}

impl Listing {
    fn of(items: Vec<Item>) -> Listing {
        Listing {
            items,
            rerun: false,
        }
    }
}

/// Everything a script filter needs.
pub struct App<'a> {
    pub runner: &'a dyn CommandRunner,
    pub gh_path: Option<&'a str>,
    pub cache: Cache,
    /// This binary, spawned detached to refresh stale entries.
    pub exe: String,
    pub home: PathBuf,
    pub settings: Settings,
}

impl<'a> App<'a> {
    pub fn gh(&self) -> Gh<'a> {
        Gh::new(self.runner, self.gh_path)
    }

    fn now(&self) -> i64 {
        time::unix(self.cache.now())
    }

    /// Serve `key` from the cache (fresh, or stale while a detached refresh
    /// runs) or fetch it synchronously when missing. Returns the value and
    /// whether a refresh is in flight. `keep` decides whether a fetched value
    /// is worth caching.
    pub fn load<T: Serialize + DeserializeOwned>(
        &self,
        key: &Key,
        fetch: impl FnOnce(&Gh) -> Result<T, GhError>,
        keep: impl Fn(&T) -> bool,
    ) -> Result<(T, bool), GhError> {
        match self.cache.read::<T>(key) {
            Entry::Fresh(value) => Ok((value, false)),
            Entry::Stale(value) => {
                if !self.cache.is_locked(key) {
                    let arg = key.to_arg();
                    // A failed spawn only means the stale value stays a little longer.
                    let _ = self
                        .runner
                        .spawn_detached(&self.exe, &["refresh", arg.as_str()]);
                }

                Ok((value, true))
            }
            Entry::Missing => {
                let value = fetch(&self.gh())?;

                if keep(&value) {
                    // Caching is an optimization: a write failure must not hide results.
                    let _ = self.cache.write(key, &value);
                }

                Ok((value, false))
            }
        }
    }

    /// Fetch `key` and rewrite its entry, holding its lock. Does nothing when
    /// another refresh holds it. On failure the lock is kept until it
    /// expires, so a failing refresh is not retried on every rerun.
    pub fn refresh(&self, key: &Key) -> Result<()> {
        let Some(lock) = self.cache.try_lock(key)? else {
            return Ok(());
        };

        let gh = self.gh();

        match key {
            Key::Viewer => self.cache.write(key, &gh.viewer_login()?)?,
            Key::Repos { owner } => {
                let fetched = repos::fetch_owner(&gh, owner)?;

                if App::keep_repos(&fetched) {
                    self.cache.write(key, &fetched)?;
                }
            }
            Key::Prs { repo, state } => self
                .cache
                .write(key, &prs::fetch_repo_prs(&gh, repo, *state)?)?,
            Key::Workflows { repo } => {
                let branch = self.default_branch(repo);
                let list = workflows::fetch_workflows(&gh, repo, branch.as_deref())?;

                self.cache.write(key, &list)?
            }
            Key::BranchRuns { repo, branch } => self
                .cache
                .write(key, &workflows::fetch_branch_runs(&gh, repo, branch)?)?,
            Key::Runs { repo, workflow_id } => self.cache.write(
                key,
                &workflows::fetch_workflow_runs(&gh, repo, *workflow_id)?,
            )?,
            Key::Search { search, qualifiers } => self
                .cache
                .write(key, &prs::fetch_search(&gh, *search, qualifiers)?)?,
        }

        lock.release();

        Ok(())
    }

    /// An organization with no repository usually means the token is not
    /// authorized for its SSO: never cache that, so authorizing shows up at
    /// once.
    fn keep_repos(fetched: &OwnerRepos) -> bool {
        fetched.kind == OwnerKind::User || !fetched.repos.is_empty()
    }

    /// The configured owners, or the authenticated user's login.
    pub fn owners(&self) -> Result<(Vec<String>, bool), GhError> {
        if !self.settings.owners.is_empty() {
            return Ok((self.settings.owners.clone(), false));
        }

        let (login, refreshing) =
            self.load::<String>(&Key::Viewer, |gh| gh.viewer_login(), |_| true)?;

        Ok((vec![login.to_lowercase()], refreshing))
    }

    /// The repositories of every owner (each fetched and cached on its own),
    /// the owners that failed, and whether a refresh is in flight.
    pub fn owner_repos(
        &self,
        owners: &[String],
    ) -> (Vec<OwnerRepos>, Vec<(String, GhError)>, bool) {
        let mut fetched = Vec::new();
        let mut failed = Vec::new();
        let mut refreshing = false;

        for owner in owners {
            let key = Key::Repos {
                owner: owner.clone(),
            };

            match self.load(&key, |gh| repos::fetch_owner(gh, owner), App::keep_repos) {
                Ok((value, stale)) => {
                    refreshing |= stale;
                    fetched.push(value);
                }
                Err(error) => failed.push((owner.clone(), error)),
            }
        }

        (fetched, failed, refreshing)
    }

    /// Items for owners that failed, plus the SSO hint for organizations that
    /// returned nothing while another owner returned repositories.
    fn owner_problems(&self, fetched: &[OwnerRepos], failed: &[(String, GhError)]) -> Vec<Item> {
        let now = self.now();
        let mut items: Vec<Item> = Vec::new();
        let mut global_shown = false;

        for (owner, error) in failed {
            // These are not about one owner: show them once.
            let global = matches!(error, GhError::Missing | GhError::NotAuthenticated);

            if global && global_shown {
                continue;
            }

            global_shown |= global;
            items.push(alfred::gh_error_item(Some(owner), error, now));
        }

        let someone_has_repos = fetched.iter().any(|owner| !owner.repos.is_empty());

        for owner in fetched {
            if owner.kind == OwnerKind::Org && owner.repos.is_empty() && someone_has_repos {
                items.push(alfred::sso_item(&owner.login));
            }
        }

        items
    }

    /// `gho`: repositories of the configured owners, ranked.
    pub fn repos(&self, input: &str) -> Listing {
        let now = self.now();

        let (owners, viewer_refreshing) = match self.owners() {
            Ok(owners) => owners,
            Err(error) => return Listing::of(vec![alfred::gh_error_item(None, &error, now)]),
        };

        let query = query::parse_repos(input, &owners);
        let selected: Vec<String> = match &query.owner {
            Some(owner) => vec![owner.clone()],
            None => owners,
        };

        let (fetched, failed, refreshing) = self.owner_repos(&selected);
        let mut items = self.owner_problems(&fetched, &failed);

        let all = fetched.into_iter().flat_map(|owner| owner.repos).collect();
        let ranked = ranking::rank_repos(all, &query.text);

        if ranked.is_empty() && items.is_empty() {
            items.push(alfred::error_item(
                "No matching repository",
                "Try another search, or refresh the cache",
            ));
        }

        items.extend(
            ranked
                .iter()
                .map(|repo| alfred::repo_item(repo, &self.settings.editor_app, now)),
        );

        if query.text.is_empty() && !ranked.is_empty() {
            items.push(alfred::refresh_item());
        }

        Listing {
            items,
            rerun: refreshing || viewer_refreshing,
        }
    }

    /// Resolve the repository typed first in a `ghp` / `gha` query. On
    /// success, the `owner/name` and the rest of the query; otherwise the
    /// listing to show instead (repositories to choose from, or an error).
    fn resolve(&self, input: &str) -> Result<(String, String), Listing> {
        let now = self.now();
        let typed = query::parse_repo(input);

        // An explicit owner/name needs no repository list.
        if typed.repo.contains('/') && local::is_valid_name_with_owner(&typed.repo) {
            let cached = self.cached_repos();

            let repo = match ranking::resolve_repo(&cached, &typed.repo) {
                Resolution::Exact(repo) => repo,
                _ => typed.repo.clone(),
            };

            return Ok((repo, typed.rest));
        }

        let (owners, viewer_refreshing) = match self.owners() {
            Ok(owners) => owners,
            Err(error) => return Err(Listing::of(vec![alfred::gh_error_item(None, &error, now)])),
        };

        let (fetched, failed, refreshing) = self.owner_repos(&owners);
        let mut items = self.owner_problems(&fetched, &failed);
        let all: Vec<_> = fetched.into_iter().flat_map(|owner| owner.repos).collect();

        match ranking::resolve_repo(&all, &typed.repo) {
            Resolution::Exact(repo) => Ok((repo, typed.rest)),
            Resolution::Candidates(candidates) => {
                items.extend(
                    candidates
                        .iter()
                        .map(|repo| alfred::candidate_item(repo, now)),
                );

                Err(Listing {
                    items,
                    rerun: refreshing || viewer_refreshing,
                })
            }
            Resolution::Unknown => {
                items.push(alfred::error_item(
                    &format!("No repository matches “{}”", typed.repo),
                    "Type owner/name, or refresh the cache from gho",
                ));

                Err(Listing {
                    items,
                    rerun: refreshing || viewer_refreshing,
                })
            }
        }
    }

    /// Repositories already cached (fresh or stale), without fetching.
    fn cached_repos(&self) -> Vec<Repo> {
        self.settings
            .owners
            .iter()
            .filter_map(|owner| {
                let key = Key::Repos {
                    owner: owner.clone(),
                };

                match self.cache.read::<OwnerRepos>(&key) {
                    Entry::Fresh(value) | Entry::Stale(value) => Some(value.repos),
                    Entry::Missing => None,
                }
            })
            .flatten()
            .collect()
    }

    /// `ghr` / `ghm`: open pull requests across the configured owners (review
    /// requested from me, or authored by me). Owner kinds, hence the
    /// `org:` / `user:` qualifiers, come from the repository cache.
    pub fn search(&self, search: Search, input: &str) -> Listing {
        let now = self.now();

        let (owners, viewer_refreshing) = match self.owners() {
            Ok(owners) => owners,
            Err(error) => return Listing::of(vec![alfred::gh_error_item(None, &error, now)]),
        };

        let (fetched, failed, repos_refreshing) = self.owner_repos(&owners);
        let mut items = self.owner_problems(&fetched, &failed);

        if fetched.is_empty() {
            return Listing {
                items,
                rerun: repos_refreshing || viewer_refreshing,
            };
        }

        let kinds: Vec<(String, OwnerKind)> = fetched
            .iter()
            .map(|owner| (owner.login.clone(), owner.kind))
            .collect();
        let qualifiers = owners::search_qualifiers(&kinds);
        let key = Key::Search {
            search,
            qualifiers: qualifiers.clone(),
        };

        let (all, refreshing) = match self.load(
            &key,
            |gh| prs::fetch_search(gh, search, &qualifiers),
            |_| true,
        ) {
            Ok(loaded) => loaded,
            Err(error) => {
                items.push(alfred::gh_error_item(None, &error, now));

                return Listing::of(items);
            }
        };

        let matching = ranking::filter_prs(all, input);

        if matching.is_empty() {
            let title = match search {
                Search::Review => "No pull request waiting for your review",
                Search::Mine => "No open pull request of yours",
            };

            items.push(alfred::error_item(title, &format!("Searched {qualifiers}")));
        }

        items.extend(matching.iter().map(|pr| alfred::pr_item(pr, true, now)));

        Listing {
            items,
            rerun: refreshing || repos_refreshing || viewer_refreshing,
        }
    }

    /// The default branch of a cached repository.
    fn default_branch(&self, repo: &str) -> Option<String> {
        self.cached_repos()
            .into_iter()
            .find(|cached| cached.name_with_owner.eq_ignore_ascii_case(repo))
            .and_then(|cached| cached.default_branch)
    }

    /// `gha`: workflows of one repository with their last run on the default
    /// branch, or the runs of one workflow (`runs:<id>`).
    pub fn workflows(&self, input: &str) -> Listing {
        let now = self.now();

        let (repo, rest) = match self.resolve(input) {
            Ok(resolved) => resolved,
            Err(listing) => return listing,
        };

        let filter = query::parse_workflow_filter(&rest);

        if let Some(workflow_id) = filter.runs_of {
            return self.runs(&repo, workflow_id, &filter.text);
        }

        let error =
            |error: GhError| Listing::of(vec![alfred::gh_error_item(Some(&repo), &error, now)]);

        let branch = self.default_branch(&repo);
        let (list, list_refreshing) = match self.load(
            &Key::Workflows { repo: repo.clone() },
            |gh| workflows::fetch_workflows(gh, &repo, branch.as_deref()),
            |_| true,
        ) {
            Ok(loaded) => loaded,
            Err(err) => return error(err),
        };

        let (runs, runs_refreshing) = match self.load(
            &Key::BranchRuns {
                repo: repo.clone(),
                branch: list.default_branch.clone(),
            },
            |gh| workflows::fetch_branch_runs(gh, &repo, &list.default_branch),
            |_| true,
        ) {
            Ok(loaded) => loaded,
            Err(err) => return error(err),
        };

        let text = filter.text.to_lowercase();
        let items: Vec<Item> = list
            .workflows
            .iter()
            .filter(|workflow| {
                workflow.name.to_lowercase().contains(&text)
                    || workflow.path.to_lowercase().contains(&text)
            })
            .map(|workflow| {
                alfred::workflow_item(
                    &repo,
                    workflow,
                    workflows::last_run(&runs, workflow.id),
                    now,
                )
            })
            .collect();

        if items.is_empty() {
            return Listing {
                items: vec![alfred::error_item(
                    &format!("No matching workflow in {repo}"),
                    "Workflows live in .github/workflows",
                )],
                rerun: list_refreshing || runs_refreshing,
            };
        }

        Listing {
            items,
            rerun: list_refreshing || runs_refreshing,
        }
    }

    /// `gha <repo> runs:<id>`: the latest runs of one workflow.
    fn runs(&self, repo: &str, workflow_id: u64, text: &str) -> Listing {
        let now = self.now();
        let key = Key::Runs {
            repo: repo.to_owned(),
            workflow_id,
        };

        let (runs, refreshing) = match self.load(
            &key,
            |gh| workflows::fetch_workflow_runs(gh, repo, workflow_id),
            |_| true,
        ) {
            Ok(loaded) => loaded,
            Err(error) => return Listing::of(vec![alfred::gh_error_item(Some(repo), &error, now)]),
        };

        let text = text.to_lowercase();
        let items: Vec<Item> = runs
            .iter()
            .filter(|run| {
                run.title.to_lowercase().contains(&text)
                    || run.branch.to_lowercase().contains(&text)
            })
            .map(|run| alfred::run_item(run, now))
            .collect();

        if items.is_empty() {
            return Listing {
                items: vec![alfred::error_item(
                    &format!("No matching run of workflow {workflow_id}"),
                    "Runs of the last 10 executions are listed",
                )],
                rerun: refreshing,
            };
        }

        Listing {
            items,
            rerun: refreshing,
        }
    }

    /// `ghp`: pull requests of one repository.
    pub fn prs(&self, input: &str) -> Listing {
        let now = self.now();

        let (repo, rest) = match self.resolve(input) {
            Ok(resolved) => resolved,
            Err(listing) => return listing,
        };

        let (state, text) = query::parse_pr_filter(&rest);
        let key = Key::Prs {
            repo: repo.clone(),
            state,
        };

        let (all, refreshing) =
            match self.load(&key, |gh| prs::fetch_repo_prs(gh, &repo, state), |_| true) {
                Ok(loaded) => loaded,
                Err(error) => {
                    return Listing::of(vec![alfred::gh_error_item(Some(&repo), &error, now)])
                }
            };

        let matching = ranking::filter_prs(all, &text);

        if matching.is_empty() {
            return Listing {
                items: vec![alfred::error_item(
                    &format!("No {} pull request in {repo}", state.as_str()),
                    "state:all or state:merged to see more",
                )],
                rerun: refreshing,
            };
        }

        Listing {
            items: matching
                .iter()
                .map(|pr| alfred::pr_item(pr, false, now))
                .collect(),
            rerun: refreshing,
        }
    }
}
