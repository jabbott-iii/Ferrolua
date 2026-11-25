//! `xtask` holds the build tasks that `cargo xtask <task>` runs, as the [quality document]
//! describes. It is a binary of the workspace that `.cargo/config.toml` runs through the `xtask`
//! alias, and it uses only the standard library and the tools of the Rust toolchain.
//!
//! # The tasks
//!
//! - `coverage`: builds the tests with coverage instrumentation, runs them, and prints the lines
//!   and regions of each crate that they ran. It exits with status 1 unless they ran all of them.
//!   The [`coverage`] module describes how.
//! - `fuzz`: builds and runs the fuzzer, `ferrolua-fuzz` in `fuzz/`, with the arguments after
//!   `fuzz`, as the [`fuzz`] module describes.
//!
//! The tasks that later roadmap tasks add are `record` (M0.5), `test` (M3.17), and `bench`
//! (M5.1), as the [roadmap] lists.
//!
//! # Output
//!
//! A task writes its report to standard output, and the output of the programs it runs, such as
//! Cargo and the tests, to standard error. `cargo xtask` exits with status 0 when the task
//! passes, 1 when it fails or stops on an error, and 2 when it isn't given a task it knows, in
//! which case it prints its usage. `fuzz` is the exception: the fuzzer writes its own report and
//! errors, and `cargo xtask` exits with the fuzzer's status.
//!
//! [quality document]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/04-quality.md#xtask
//! [roadmap]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/roadmap.md

mod cargo;
mod coverage;
mod error;
mod fuzz;
mod json;
mod llvm;
mod process;
mod report;

use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

/// What `cargo xtask` prints when it isn't given a task it knows.
const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  coverage  run the tests with coverage instrumentation, and fail unless they run every line
            and region
  fuzz      run the fuzzer with the arguments after `fuzz`, which `cargo xtask fuzz` alone
            lists
";

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

/// What a task printed, and the status that `cargo xtask` exits with.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    stdout: String,
    stderr: String,
    status: u8,
}

/// Runs the task that `args`, the arguments after `cargo xtask`, name.
fn run(args: &[OsString]) -> Outcome {
    match args {
        [task] if task == "coverage" => match coverage::run() {
            Ok(report) => Outcome {
                stdout: report.render(),
                stderr: String::new(),
                status: if report.passed() { 0 } else { 1 },
            },
            Err(error) => Outcome {
                stdout: String::new(),
                stderr: format!("error: {error}\n"),
                status: 1,
            },
        },
        [task, args @ ..] if task == "fuzz" => match fuzz::run(args) {
            Ok(status) => Outcome {
                stdout: String::new(),
                stderr: String::new(),
                status,
            },
            Err(error) => Outcome {
                stdout: String::new(),
                stderr: format!("error: {error}\n"),
                status: 1,
            },
        },
        _ => Outcome {
            stdout: String::new(),
            stderr: USAGE.to_owned(),
            status: 2,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Outcome, USAGE, run};

    fn usage() -> Outcome {
        Outcome {
            stdout: String::new(),
            stderr: USAGE.to_owned(),
            status: 2,
        }
    }

    #[test]
    fn prints_usage_without_a_task() {
        assert_eq!(run(&[]), usage());
    }

    #[test]
    fn prints_usage_for_an_unknown_task() {
        assert_eq!(run(&[OsString::from("cover")]), usage());
    }

    #[test]
    fn prints_usage_for_a_task_with_arguments() {
        assert_eq!(
            run(&[OsString::from("coverage"), OsString::from("--all")]),
            usage()
        );
    }
}
