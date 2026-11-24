//! Parsing of what the user types after each keyword.

use crate::owners;

/// `gho [<owner>:]<text>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReposQuery {
    /// Restrict to this owner (a login from `GH_OWNERS`).
    pub owner: Option<String>,
    /// What to rank repositories by.
    pub text: String,
}

/// Parse a `gho` query. A leading `x:` (first letter of an owner) or
/// `login:` restricts the list to that owner; an unknown or ambiguous prefix
/// is searched as text.
pub fn parse_repos(query: &str, owners: &[String]) -> ReposQuery {
    let query = query.trim();

    if let Some((prefix, rest)) = query.split_once(':') {
        if !prefix.contains(char::is_whitespace) {
            if let Some(owner) = owners::resolve_prefix(prefix, owners) {
                return ReposQuery {
                    owner: Some(owner.clone()),
                    text: rest.trim().to_owned(),
                };
            }
        }
    }

    ReposQuery {
        owner: None,
        text: query.to_owned(),
    }
}
