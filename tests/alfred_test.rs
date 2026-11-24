//! Tests for the JSON Alfred receives from script filters.

use std::time::Duration;

use alfred_gh::alfred;
use serde_json::Value;

fn json(output: &powerpack::Output) -> Value {
    let mut buffer = Vec::new();
    output.write(&mut buffer).unwrap();

    serde_json::from_slice(&buffer).unwrap()
}

#[test]
fn error_items_are_not_actionable() {
    let output = alfred::output(vec![alfred::error_item("Oops", "details")], None);
    let item = &json(&output)["items"][0];

    assert_eq!(item["title"], "Oops");
    assert_eq!(item["subtitle"], "details");
    assert_eq!(item["valid"], false);
    assert!(item.get("uid").is_none());
}

#[test]
fn rerun_is_only_set_when_asked() {
    let without = json(&alfred::output(Vec::new(), None));
    let with = json(&alfred::output(
        Vec::new(),
        Some(Duration::from_millis(500)),
    ));

    assert!(without.get("rerun").is_none());
    assert_eq!(with["rerun"], 0.5);
}
