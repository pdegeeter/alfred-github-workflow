//! The owners (organizations and users) whose repositories are listed.
//!
//! They come from the `GH_OWNERS` workflow variable. Whether an owner is an
//! organization or a user is learnt from GitHub (`repositoryOwner.__typename`)
//! and cached with its repositories, never hardcoded.

use serde::{Deserialize, Serialize};

/// Whether an owner is an organization or a user account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnerKind {
    Org,
    User,
}

impl OwnerKind {
    /// From GraphQL's `__typename`.
    pub fn from_typename(typename: &str) -> Option<OwnerKind> {
        match typename {
            "Organization" => Some(OwnerKind::Org),
            "User" => Some(OwnerKind::User),
            _ => None,
        }
    }

    /// Search qualifier restricting results to this kind of owner.
    pub fn qualifier(self) -> &'static str {
        match self {
            OwnerKind::Org => "org",
            OwnerKind::User => "user",
        }
    }
}

/// Parse `GH_OWNERS`: comma-separated logins, trimmed, lowercased (logins are
/// case-insensitive) and deduplicated in order.
pub fn parse(raw: &str) -> Vec<String> {
    let mut owners: Vec<String> = Vec::new();

    for login in raw.split(',').map(|login| login.trim().to_lowercase()) {
        if !login.is_empty() && !owners.contains(&login) {
            owners.push(login);
        }
    }

    owners
}

/// Search qualifiers such as `org:acme-org user:octo-user`, from owners whose
/// kind is known.
pub fn search_qualifiers(owners: &[(String, OwnerKind)]) -> String {
    owners
        .iter()
        .map(|(login, kind)| format!("{}:{login}", kind.qualifier()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve an owner filter typed before a colon (`gho a:api`): the full login,
/// or its first letter when no other owner shares it.
pub fn resolve_prefix<'a>(prefix: &str, owners: &'a [String]) -> Option<&'a String> {
    let prefix = prefix.to_lowercase();

    if prefix.is_empty() {
        return None;
    }

    if let Some(owner) = owners.iter().find(|owner| **owner == prefix) {
        return Some(owner);
    }

    if prefix.chars().count() != 1 {
        return None;
    }

    let mut matching = owners.iter().filter(|owner| owner.starts_with(&prefix));
    let first = matching.next()?;

    if matching.next().is_some() {
        return None;
    }

    Some(first)
}
