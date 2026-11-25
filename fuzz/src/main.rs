//! `lunaferra-fuzz` is Lunaferra's fuzzer: it feeds its targets, such as the parser and the VM,
//! inputs mutated from a fixed seed, and saves every input that makes one panic, crash, or hang, as
//! the [quality document] describes. ADR-0002 (Lunaferra uses no third-party code) rules out
//! cargo-fuzz and libFuzzer, so this program takes their place. It uses only the standard library
//! and the seeded generator of `lunaferra-testkit`.
//!
//! `cargo xtask fuzz` builds and runs it, passing on its arguments; [`cli::usage`] gives them.
//!
//! # How a campaign runs
//!
//! 1. [`campaign::run`] reads the target's corpus from `fuzz/corpus/<target>/`, and makes the
//!    input of each run with [`mutate`](mutate::mutate), from the seed `seed + run`.
//! 2. [`runner::Runner`] starts this program again as a child process for each batch of inputs.
//!    The child, in [`child`], runs the target on each input, so a crash or a hang ends only the
//!    child. The runner stops a child once an input runs past the time limit.
//! 3. At the first input that fails, [`shrink`](shrink::shrink) runs smaller inputs, each in a
//!    child of its own, until it finds the smallest one it can that fails the same way.
//! 4. [`folder::save`] saves it as `fuzz/regressions/<target>/<hash>.bin`, and the report gives
//!    the command that replays it.
//!
//! `cargo test` runs `--replay-saved`, which runs every saved input again, in [`replay`].
//!
//! The fuzzer isn't guided by coverage, because stable Rust can't read coverage counters while a
//! program runs. Each run starts from the target's corpus, so a corpus input close to the inputs
//! that reach deep code does the work that coverage guidance would.
//!
//! # Output
//!
//! The fuzzer writes its report to standard output, and its errors to standard error. It exits
//! with status 0 when no run failed, when every saved input ran as it should, or after it lists
//! the targets with `--list`; 1 when a run
//! failed, when a saved input didn't run as it should, or on an error; and 2, after printing its
//! usage, when it isn't given a command it knows. `--replay` exits with status 101 when the input
//! panics, as Rust does for any program that a panic ends.
//!
//! [quality document]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/04-quality.md#lunaferra-fuzz

mod batch;
mod campaign;
mod child;
mod cli;
mod error;
mod folder;
mod mutate;
mod replay;
mod runner;
mod shrink;
mod target;

use std::env;
use std::ffi::OsString;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::process::{self, ExitCode};

use crate::cli::Command;
use crate::error::Error;

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the entry point writes the report and the errors"
)]
fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let outcome = run(&args);
    print!("{}", outcome.stdout);
    eprint!("{}", outcome.stderr);
    ExitCode::from(outcome.status)
}

/// What a command printed, and the status that the fuzzer exits with.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    stdout: String,
    stderr: String,
    status: u8,
}

impl Outcome {
    fn report(stdout: String, status: u8) -> Outcome {
        Outcome {
            stdout,
            stderr: String::new(),
            status,
        }
    }

    fn error(error: &Error) -> Outcome {
        Outcome {
            stdout: String::new(),
            stderr: format!("error: {error}\n"),
            status: 1,
        }
    }
}

/// Runs the command that `args`, the arguments after the program's name, name.
fn run(args: &[OsString]) -> Outcome {
    match cli::parse(args) {
        Some(Command::Fuzz(options)) => {
            match env::current_exe()
                .map_err(Error::CurrentExe)
                .and_then(|exe| campaign::run(&exe, work(), &options))
            {
                Ok(report) => Outcome::report(report.to_string(), report.status()),
                Err(error) => Outcome::error(&error),
            }
        }
        Some(Command::ReplaySaved {
            regressions,
            time_limit,
        }) => match env::current_exe()
            .map_err(Error::CurrentExe)
            .and_then(|exe| replay::saved(&exe, work(), &regressions, time_limit))
        {
            Ok(replayed) => Outcome::report(replayed.to_string(), replayed.status()),
            Err(error) => Outcome::error(&error),
        },
        Some(Command::Replay { target, path }) => match replay::one(target, &path) {
            Ok(()) => Outcome::report(format!("{}: {} passed\n", target.name, path.display()), 0),
            Err(error) => Outcome::error(&error),
        },
        Some(Command::List) => Outcome::report(target::list(target::TARGETS), 0),
        Some(Command::Child(target)) => {
            // The child reports each panic's message to its parent, so the hook prints nothing.
            panic::set_hook(Box::new(|_| {}));
            match child::run(target, &mut io::stdin().lock(), &mut io::stdout().lock()) {
                Ok(ran) => Outcome::report(String::new(), ran.status()),
                Err(error) => Outcome::error(&error),
            }
        }
        None => Outcome {
            stdout: String::new(),
            stderr: cli::usage(),
            status: 2,
        },
    }
}

/// The work folder of this process's runner, in the system's temporary folder. It is named after
/// the process, so two fuzzers that run at the same time don't share one.
fn work() -> PathBuf {
    env::temp_dir().join(format!("lunaferra-fuzz-{}", process::id()))
}
