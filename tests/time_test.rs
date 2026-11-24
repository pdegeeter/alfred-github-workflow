//! Tests for timestamp parsing and relative ages.

use alfred_gh::time;

#[test]
fn parses_github_timestamps() {
    assert_eq!(time::parse_iso8601("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(
        time::parse_iso8601("2026-11-24T16:54:22Z"),
        Some(1_795_539_262)
    );
    assert_eq!(
        time::parse_iso8601("2024-02-29T12:00:00.123Z"),
        Some(1_709_208_000)
    );
}

#[test]
fn rejects_malformed_timestamps() {
    for value in [
        "",
        "2026-11-24",
        "2026-11-24T16:54:22",
        "2026-13-01T00:00:00Z",
        "garbage",
    ] {
        assert_eq!(time::parse_iso8601(value), None, "{value:?}");
    }
}

#[test]
fn ages_are_compact() {
    assert_eq!(time::ago(0, 30), "just now");
    assert_eq!(time::ago(0, 5 * 60), "5m ago");
    assert_eq!(time::ago(0, 3 * 3_600), "3h ago");
    assert_eq!(time::ago(0, 2 * 86_400), "2d ago");
    assert_eq!(time::ago(0, 70 * 86_400), "2mo ago");
    assert_eq!(time::ago(0, 400 * 86_400), "1y ago");
    assert_eq!(time::ago(100, 0), "just now", "future dates clamp");
    assert_eq!(time::ago_iso("garbage", 0), "");
}

#[test]
fn durations_are_compact() {
    assert_eq!(time::duration(45), "45s");
    assert_eq!(time::duration(192), "3m 12s");
    assert_eq!(time::duration(3_900), "1h 05m");
    assert_eq!(time::duration(-3), "0s");
}
