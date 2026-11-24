//! Script filter items and output.
//!
//! Every actionable item (and each of its modifiers) carries an
//! [`Action`](crate::action::Action) encoded in its `arg`; the one connected
//! Run Script decodes and performs it. No `uid` is set: Alfred would otherwise
//! re-sort items by its own usage knowledge and override the order computed
//! here.

use std::time::Duration;

use powerpack::{Item, Output};

/// Item shown in place of a list when something went wrong.
pub fn error_item(title: &str, message: &str) -> Item {
    Item::new(title).subtitle(message).valid(false)
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
