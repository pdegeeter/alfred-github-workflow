//! Jira ticket keys (`ABC-123`) in pull request titles and branch names.

/// The first ticket key matching `[A-Z][A-Z0-9]+-\d+` in `text`. A key must
/// not continue an uppercase word or a number (`XABC-1` is not `ABC-1`).
pub fn find_key(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();

    for start in 0..chars.len() {
        if !chars[start].is_ascii_uppercase() {
            continue;
        }

        let continues_word = start > 0
            && (chars[start - 1].is_ascii_uppercase() || chars[start - 1].is_ascii_digit());

        if continues_word {
            continue;
        }

        let mut end = start + 1;

        while end < chars.len() && (chars[end].is_ascii_uppercase() || chars[end].is_ascii_digit())
        {
            end += 1;
        }

        if end - start < 2 || end >= chars.len() || chars[end] != '-' {
            continue;
        }

        let digits_start = end + 1;
        let mut digits_end = digits_start;

        while digits_end < chars.len() && chars[digits_end].is_ascii_digit() {
            digits_end += 1;
        }

        if digits_end > digits_start {
            return Some(chars[start..digits_end].iter().collect());
        }
    }

    None
}

/// The ticket key of a pull request: from its title, else its head branch.
pub fn pr_key(title: &str, head: &str) -> Option<String> {
    find_key(title).or_else(|| find_key(head))
}
