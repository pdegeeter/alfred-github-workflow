//! Tests for the stale-while-revalidate cache: freshness from an injected
//! clock, corrupted files, locks and keys.

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alfred_gh::cache::{Cache, Entry, Key};

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn repos_key() -> Key {
    Key::Repos {
        owner: "acme-org".to_owned(),
    }
}

#[test]
fn a_missing_entry_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path().join("not-created-yet"), at(1_000));

    assert_eq!(cache.read::<String>(&repos_key()), Entry::Missing);
}

#[test]
fn an_entry_is_fresh_within_its_ttl_and_stale_after() {
    let dir = tempfile::tempdir().unwrap();
    Cache::new(dir.path(), at(1_000))
        .write(&repos_key(), &"value")
        .unwrap();

    let read_at = |seconds| Cache::new(dir.path(), at(seconds)).read::<String>(&repos_key());

    assert_eq!(read_at(1_000), Entry::Fresh("value".to_owned()));
    assert_eq!(read_at(1_000 + 3_599), Entry::Fresh("value".to_owned()));
    assert_eq!(read_at(1_000 + 3_600), Entry::Stale("value".to_owned()));
    assert_eq!(
        read_at(999),
        Entry::Stale("value".to_owned()),
        "clock went back"
    );
}

#[test]
fn a_corrupted_or_mistyped_entry_counts_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path(), at(1_000));

    fs::write(dir.path().join("repos-acme-org.json"), "{not json").unwrap();
    assert_eq!(cache.read::<String>(&repos_key()), Entry::Missing);

    cache.write(&repos_key(), &42).unwrap();
    assert_eq!(cache.read::<String>(&repos_key()), Entry::Missing);
}

#[test]
fn writes_leave_no_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    Cache::new(dir.path(), at(1_000))
        .write(&repos_key(), &"value")
        .unwrap();

    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();

    assert_eq!(names, ["repos-acme-org.json"]);
}

#[test]
fn a_lock_is_exclusive_until_released() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path(), at(1_000));

    let lock = cache.try_lock(&repos_key()).unwrap().expect("first lock");
    assert!(cache.is_locked(&repos_key()));
    assert!(cache.try_lock(&repos_key()).unwrap().is_none());

    lock.release();
    assert!(!cache.is_locked(&repos_key()));
    assert!(cache.try_lock(&repos_key()).unwrap().is_some());
}

#[test]
fn an_abandoned_lock_expires() {
    let dir = tempfile::tempdir().unwrap();
    let lock = Cache::new(dir.path(), at(1_000))
        .try_lock(&repos_key())
        .unwrap()
        .unwrap();
    drop(lock);

    let later = Cache::new(dir.path(), at(1_000 + 59));
    assert!(later.is_locked(&repos_key()));

    let expired = Cache::new(dir.path(), at(1_000 + 60));
    assert!(!expired.is_locked(&repos_key()));
    assert!(expired.try_lock(&repos_key()).unwrap().is_some());
}

#[test]
fn clear_removes_entries_and_locks() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path(), at(1_000));
    cache.write(&Key::Viewer, &"octo-user").unwrap();
    cache.write(&repos_key(), &"value").unwrap();
    let _lock = cache.try_lock(&repos_key()).unwrap();

    assert_eq!(cache.clear().unwrap(), 3);
    assert_eq!(cache.read::<String>(&Key::Viewer), Entry::Missing);
    assert!(!cache.is_locked(&repos_key()));
}

#[test]
fn keys_round_trip_and_make_safe_file_names() {
    for key in [Key::Viewer, repos_key()] {
        assert_eq!(Key::parse(&key.to_arg()).unwrap(), key);
    }

    assert_eq!(
        Key::Repos {
            owner: "../evil".to_owned()
        }
        .stem(),
        "repos-.._evil"
    );
    assert!(Key::parse("repos").is_err());
    assert!(Key::parse("viewer extra").is_err());
    assert!(Key::parse("nope").is_err());
}

#[test]
fn ttls_follow_the_kind_of_data() {
    assert_eq!(Key::Viewer.ttl(), Duration::from_secs(3_600));
    assert_eq!(repos_key().ttl(), Duration::from_secs(3_600));
}
