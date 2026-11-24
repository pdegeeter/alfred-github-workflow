//! `alfred-gh` — Alfred workflow entry point.
//!
//! Subcommands, dispatched by Alfred:
//!   * `run <arg>` — Run Script action: perform the selected item.

use std::env;
use std::process::ExitCode;

use alfred_gh::action::{self, Action};
use alfred_gh::runner::{CommandRunner, SystemRunner};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let arg = args.next().unwrap_or_default();
    let runner = SystemRunner;

    match command.as_str() {
        "run" => run(&runner, &arg),
        other => {
            eprintln!("unknown command: `{other}` (expected run)");

            ExitCode::FAILURE
        }
    }
}

/// Run Script action: decode the item's `arg` and perform it. Whatever is
/// printed becomes the text of the connected notification.
fn run(runner: &dyn CommandRunner, arg: &str) -> ExitCode {
    let result = Action::parse(arg).and_then(|parsed| action::execute(runner, &parsed));

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
