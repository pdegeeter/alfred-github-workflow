//! Every GitHub call goes through the `gh` CLI, which handles authentication,
//! token refresh and SAML SSO. The workflow never sees a token.
//!
//! GraphQL queries are fixed constants; variables travel as separate
//! `-f name=value` arguments (raw strings: `-F` would read `@file` values and
//! coerce types), never interpolated into the query.

use std::fmt;
use std::path::Path;

use anyhow::anyhow;
use serde_json::Value;

use crate::runner::{CommandError, CommandRunner};

/// Where `gh` is looked for. Alfred does not load the user's shell `PATH`, so
/// the binary is resolved by absolute path (Apple Silicon Homebrew first).
pub const GH_PATHS: [&str; 2] = ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"];

/// Settings page where the user authorizes `gh` for SAML organizations.
pub const SSO_SETTINGS_URL: &str = "https://github.com/settings/connections/applications";

/// Query of the authenticated user's login.
pub const VIEWER_QUERY: &str = "query { viewer { login } }";

/// The first existing entry of [`GH_PATHS`].
pub fn resolve(exists: &dyn Fn(&Path) -> bool) -> Option<&'static str> {
    GH_PATHS
        .into_iter()
        .find(|candidate| exists(Path::new(candidate)))
}

/// Why a `gh` call failed, classified so the user gets the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhError {
    /// `gh` is not installed at any of [`GH_PATHS`].
    Missing,
    /// `gh` has no valid credentials.
    NotAuthenticated,
    /// The API rate limit is exhausted; `reset` is its Unix reset time.
    RateLimited { reset: Option<i64> },
    /// The token is not authorized for a SAML-protected organization.
    SsoRequired,
    /// Anything else, with a readable message.
    Other(String),
}

impl fmt::Display for GhError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GhError::Missing => write!(f, "GitHub CLI not found"),
            GhError::NotAuthenticated => write!(f, "GitHub CLI is not authenticated"),
            GhError::RateLimited { .. } => write!(f, "GitHub API rate limit reached"),
            GhError::SsoRequired => write!(f, "token not authorized for SSO"),
            GhError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for GhError {}

/// A GraphQL variable passed as `-f name=value`.
pub type Var<'a> = (&'a str, &'a str);

/// A `gh` binary and the runner that spawns it.
pub struct Gh<'a> {
    runner: &'a dyn CommandRunner,
    path: Option<&'a str>,
}

impl<'a> Gh<'a> {
    pub fn new(runner: &'a dyn CommandRunner, path: Option<&'a str>) -> Self {
        Self { runner, path }
    }

    /// The resolved `gh` path, or [`GhError::Missing`].
    pub fn path(&self) -> Result<&'a str, GhError> {
        self.path.ok_or(GhError::Missing)
    }

    /// The argv of a GraphQL call (after the program).
    pub fn graphql_args(query: &str, vars: &[Var]) -> Vec<String> {
        let mut args = vec![
            "api".to_owned(),
            "graphql".to_owned(),
            "-f".to_owned(),
            format!("query={query}"),
        ];

        for (name, value) in vars {
            args.push("-f".to_owned());
            args.push(format!("{name}={value}"));
        }

        args
    }

    /// Run a GraphQL query and return its `data`. GraphQL errors are
    /// classified even when `gh` exits successfully.
    pub fn graphql(&self, query: &str, vars: &[Var]) -> Result<Value, GhError> {
        let args = Gh::graphql_args(query, vars);
        let stdout = self.call(&args)?;
        let body: Value = serde_json::from_str(&stdout)
            .map_err(|err| GhError::Other(format!("invalid GraphQL response: {err}")))?;

        if let Some(error) = Gh::classify_body(&body) {
            return Err(self.with_reset(error));
        }

        Ok(body["data"].clone())
    }

    /// Run a REST `GET` on `path` (for example `repos/o/r/actions/workflows`).
    pub fn rest(&self, path: &str) -> Result<Value, GhError> {
        let args = vec!["api".to_owned(), path.to_owned()];
        let stdout = self.call(&args)?;

        serde_json::from_str(&stdout)
            .map_err(|err| GhError::Other(format!("invalid REST response: {err}")))
    }

    /// The login of the authenticated user.
    pub fn viewer_login(&self) -> Result<String, GhError> {
        let data = self.graphql(VIEWER_QUERY, &[])?;

        data["viewer"]["login"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| GhError::Other("no viewer login in the response".to_owned()))
    }

    fn call(&self, args: &[String]) -> Result<String, GhError> {
        let path = self.path()?;
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();

        match self.runner.run(path, &argv) {
            Ok(stdout) => Ok(stdout),
            Err(err) => {
                let error = match err.downcast_ref::<CommandError>() {
                    Some(failure) => Gh::classify(&failure.stdout, &failure.stderr),
                    None => GhError::Other(format!("{:#}", anyhow!(err))),
                };

                Err(self.with_reset(error))
            }
        }
    }

    /// Look up the reset time of an exhausted rate limit (`rate_limit` does
    /// not count against it). Best effort: the time stays unknown on failure.
    fn with_reset(&self, error: GhError) -> GhError {
        if error != (GhError::RateLimited { reset: None }) {
            return error;
        }

        let Some(path) = self.path else {
            return error;
        };

        let reset = self
            .runner
            .run(path, &["api", "rate_limit"])
            .ok()
            .and_then(|stdout| serde_json::from_str::<Value>(&stdout).ok())
            .and_then(|body| Gh::earliest_exhausted_reset(&body));

        GhError::RateLimited { reset }
    }

    fn earliest_exhausted_reset(body: &Value) -> Option<i64> {
        body["resources"]
            .as_object()?
            .values()
            .filter(|resource| resource["remaining"].as_i64() == Some(0))
            .filter_map(|resource| resource["reset"].as_i64())
            .min()
    }

    /// Classify a failed `gh` run from its stdout (GraphQL JSON, if any) and
    /// stderr.
    pub fn classify(stdout: &str, stderr: &str) -> GhError {
        if let Some(error) = serde_json::from_str::<Value>(stdout)
            .ok()
            .and_then(|body| Gh::classify_body(&body))
        {
            return error;
        }

        let lower = stderr.to_lowercase();

        if lower.contains("gh auth login")
            || lower.contains("http 401")
            || lower.contains("bad credentials")
        {
            return GhError::NotAuthenticated;
        }

        if lower.contains("rate limit") {
            return GhError::RateLimited { reset: None };
        }

        if lower.contains("saml") {
            return GhError::SsoRequired;
        }

        let message = stderr.trim().trim_start_matches("gh: ").trim();

        if message.is_empty() {
            return GhError::Other("gh failed without a message".to_owned());
        }

        GhError::Other(message.to_owned())
    }

    /// The error carried by a GraphQL response body, if any.
    pub fn classify_body(body: &Value) -> Option<GhError> {
        let errors = body["errors"]
            .as_array()
            .filter(|errors| !errors.is_empty())?;

        let is_sso = errors.iter().any(|error| {
            error["type"] == "FORBIDDEN"
                || error["extensions"]["saml_failure"] == true
                || error["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("SAML"))
        });

        if is_sso {
            return Some(GhError::SsoRequired);
        }

        if errors.iter().any(|error| error["type"] == "RATE_LIMITED") {
            return Some(GhError::RateLimited { reset: None });
        }

        let message = errors[0]["message"]
            .as_str()
            .unwrap_or("unknown GraphQL error");

        Some(GhError::Other(message.to_owned()))
    }
}
