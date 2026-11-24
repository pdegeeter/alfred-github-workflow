//! `alfred-gh` — Alfred workflow entry point.
//!
//! Subcommands, dispatched by Alfred:
//!   * `repos [query]` — script filter `gho`: repositories of the configured owners.
//!   * `run <arg>`     — Run Script action: perform the selected item.
//!   * `refresh <key>` — rewrite one stale cache entry (spawned detached by the
//!     script filters, never by Alfred directly).

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::SystemTime;

use alfred_gh::action::{self, Action, Context};
use alfred_gh::alfred;
use alfred_gh::app::{App, Listing, Settings};
use alfred_gh::cache::{Cache, Key};
use alfred_gh::gh;
use alfred_gh::runner::SystemRunner;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let arg = args.next().unwrap_or_default();
    let runner = SystemRunner;
    let gh_path = gh::resolve(&|path: &Path| path.exists());

    let app = App {
        runner: &runner,
        gh_path,
        cache: Cache::new(
            powerpack::env::workflow_cache_or_default(),
            SystemTime::now(),
        ),
        exe: env::current_exe()
            .map(|exe| exe.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "./alfred-gh".to_owned()),
        home: env::var_os("HOME").map(PathBuf::from).unwrap_or_default(),
        settings: Settings::from_vars(|name| env::var(name).ok()),
    };

    match command.as_str() {
        "repos" => output(app.repos(&arg)),
        "run" => run(&app, &arg),
        "refresh" => refresh(&app, &arg),
        other => {
            eprintln!("unknown command: `{other}` (expected repos, run or refresh)");

            ExitCode::FAILURE
        }
    }
}

/// Print script filter items.
fn output(listing: Listing) -> ExitCode {
    let rerun = listing.rerun.then_some(alfred::RERUN);

    match alfred::output(listing.items, rerun).write(std::io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("failed to write Alfred output: {err}");

            ExitCode::FAILURE
        }
    }
}

/// Run Script action: decode the item's `arg` and perform it. Whatever is
/// printed becomes the text of the connected notification.
fn run(app: &App, arg: &str) -> ExitCode {
    let ctx = Context {
        runner: app.runner,
        gh: app.gh_path,
        home: app.home.clone(),
        cache: &app.cache,
        exists: &|path: &Path| path.exists(),
    };

    let result = Action::parse(arg).and_then(|parsed| action::execute(&ctx, &parsed));

    match result {
        Ok(message) => {
            if let Some(message) = message {
                println!("{message}");
            }

            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("GitHub: {err}");
            eprintln!("{err:#}");

            ExitCode::FAILURE
        }
    }
}

fn refresh(app: &App, arg: &str) -> ExitCode {
    match Key::parse(arg).and_then(|key| app.refresh(&key)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:#}");

            ExitCode::FAILURE
        }
    }
}
