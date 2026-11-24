//! Local clones live at `~/Workspace/github.com/<owner>/<repo>`.

use std::path::{Path, PathBuf};

/// Whether `value` is a plain `owner/repo`: two non-empty segments of
/// letters, digits, `-`, `_` and `.`, none starting with `-` or `.` (so it can
/// be neither a flag for `gh` nor a path escape).
pub fn is_valid_name_with_owner(value: &str) -> bool {
    let Some((owner, name)) = value.split_once('/') else {
        return false;
    };

    [owner, name].iter().all(|segment| {
        !segment.is_empty()
            && !segment.starts_with(['-', '.'])
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    })
}

/// Where the clone of `owner/repo` lives under `home`.
pub fn clone_path(home: &Path, name_with_owner: &str) -> PathBuf {
    let mut path = home.join("Workspace").join("github.com");

    for segment in name_with_owner.split('/') {
        path.push(segment);
    }

    path
}
