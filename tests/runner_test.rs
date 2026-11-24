//! Tests for [`SystemRunner`] against real, harmless system programs.

use alfred_gh::runner::{CommandError, CommandRunner, SystemRunner};

#[test]
fn run_returns_stdout_on_success() {
    let stdout = SystemRunner.run("/bin/echo", &["hello"]).unwrap();

    assert_eq!(stdout, "hello\n");
}

#[test]
fn a_failure_keeps_stdout_stderr_and_the_exit_code() {
    let err = SystemRunner
        .run(
            "/bin/sh",
            &["-c", "echo '{\"errors\":[]}'; echo bad >&2; exit 3"],
        )
        .unwrap_err();

    let failure = err.downcast_ref::<CommandError>().expect("a CommandError");

    assert_eq!(failure.status, Some(3));
    assert_eq!(failure.stdout, "{\"errors\":[]}\n");
    assert_eq!(failure.stderr, "bad\n");
    assert_eq!(err.to_string(), "`/bin/sh` exited with 3: bad");
}

#[test]
fn a_missing_program_is_an_error() {
    assert!(SystemRunner.run("/no/such/program", &[]).is_err());
    assert!(SystemRunner
        .spawn_detached("/no/such/program", &[])
        .is_err());
}

#[test]
fn spawn_detached_returns_without_waiting() {
    let started = std::time::Instant::now();

    SystemRunner.spawn_detached("/bin/sleep", &["5"]).unwrap();

    assert!(started.elapsed().as_secs() < 2);
}
