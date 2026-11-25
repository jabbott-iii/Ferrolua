//! The command line of `lunaferra-fuzz`, which `cargo xtask fuzz` passes its arguments to.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::campaign::Options;
use crate::target::{self, TARGETS, Target};

/// How many runs a campaign makes unless `--runs` says otherwise.
pub(crate) const DEFAULT_RUNS: u64 = 100_000;

/// How many milliseconds one input may run unless `--time-limit` says otherwise.
pub(crate) const DEFAULT_TIME_LIMIT_MS: u64 = 1000;

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Run a campaign.
    Fuzz(Options),
    /// Run one input in this process.
    Replay {
        /// The target to run.
        target: &'static Target,
        /// The input's file.
        path: PathBuf,
    },
    /// Run every saved input, each in a child process.
    ReplaySaved {
        /// The folder of saved inputs.
        regressions: PathBuf,
        /// How long one input may run.
        time_limit: Duration,
    },
    /// Run the batch on standard input as a child process of a campaign or a replay.
    Child(&'static Target),
}

/// Reads the command from `args`, the arguments after the program's name, or returns `None` when
/// they aren't a command it knows.
pub(crate) fn parse(args: &[OsString]) -> Option<Command> {
    match args {
        [flag, name] if flag == "--child" => find(name).map(Command::Child),
        [flag, rest @ ..] if flag == "--replay-saved" => {
            let flags = flags(rest, &["--regressions", "--time-limit"])?;
            Some(Command::ReplaySaved {
                regressions: folder(&flags, "--regressions", "regressions"),
                time_limit: time_limit(&flags)?,
            })
        }
        [name, flag, path] if flag == "--replay" => Some(Command::Replay {
            target: find(name)?,
            path: PathBuf::from(path),
        }),
        [name, rest @ ..] => {
            let target = find(name)?;
            let flags = flags(
                rest,
                &[
                    "--seed",
                    "--runs",
                    "--time-limit",
                    "--corpus",
                    "--regressions",
                ],
            )?;
            Some(Command::Fuzz(Options {
                target,
                seed: number(&flags, "--seed", 0)?,
                runs: number(&flags, "--runs", DEFAULT_RUNS)?,
                time_limit: time_limit(&flags)?,
                corpus: folder(&flags, "--corpus", "corpus"),
                regressions: folder(&flags, "--regressions", "regressions"),
            }))
        }
        [] => None,
    }
}

/// The text `cargo xtask fuzz` prints when it isn't given a command it knows.
pub(crate) fn usage() -> String {
    let width = TARGETS
        .iter()
        .map(|target| target.name.len())
        .max()
        .unwrap_or(0);
    let targets: String = TARGETS
        .iter()
        .map(|target| format!("  {:width$}  {}\n", target.name, target.about))
        .collect();
    format!(
        "\
usage: cargo xtask fuzz <target> [--seed <n>] [--runs <n>] [--time-limit <ms>]
                        [--corpus <folder>] [--regressions <folder>]
       cargo xtask fuzz <target> --replay <file>
       cargo xtask fuzz --replay-saved [--regressions <folder>] [--time-limit <ms>]

The fuzzer runs <target> on inputs mutated from its corpus until one panics, crashes, or runs
past the time limit. Then it shrinks that input, saves it, and stops.

  --seed <n>              the seed of the first run; run i uses seed <n> + i (default 0)
  --runs <n>              how many inputs to run (default {DEFAULT_RUNS})
  --time-limit <ms>       how long one input may run (default {DEFAULT_TIME_LIMIT_MS})
  --corpus <folder>       the folder of each target's corpus folder (default fuzz/corpus)
  --regressions <folder>  the folder of each target's saved inputs (default fuzz/regressions)
  --replay <file>         run <target> on one input, in this process
  --replay-saved          run every saved input, and check that each runs as it should

targets:
{targets}"
    )
}

/// The target named `name`.
fn find(name: &OsStr) -> Option<&'static Target> {
    name.to_str().and_then(target::find)
}

/// Reads `args` as pairs of a flag from `allowed` and its value. Returns `None` for a flag that
/// isn't allowed, a flag given twice, or a flag without a value.
fn flags<'a>(
    args: &'a [OsString],
    allowed: &[&'static str],
) -> Option<BTreeMap<&'static str, &'a OsStr>> {
    let mut flags = BTreeMap::new();
    let mut rest = args;
    while let [flag, value, tail @ ..] = rest {
        let name = allowed.iter().find(|name| flag == **name)?;
        if flags.insert(*name, value.as_os_str()).is_some() {
            return None;
        }
        rest = tail;
    }
    rest.is_empty().then_some(flags)
}

/// The number that flag `name` gives, or `default` when it isn't given. Returns `None` when the
/// value isn't a number from 0 to `u64::MAX`.
fn number(flags: &BTreeMap<&str, &OsStr>, name: &str, default: u64) -> Option<u64> {
    flags.get(name).map_or(Some(default), |value| {
        value.to_str().and_then(|value| value.parse().ok())
    })
}

/// The time limit that `--time-limit` gives in milliseconds.
fn time_limit(flags: &BTreeMap<&str, &OsStr>) -> Option<Duration> {
    number(flags, "--time-limit", DEFAULT_TIME_LIMIT_MS).map(Duration::from_millis)
}

/// The folder that flag `name` gives, or the folder `default` in `fuzz/` when it isn't given.
fn folder(flags: &BTreeMap<&str, &OsStr>, name: &str, default: &str) -> PathBuf {
    flags.get(name).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join(default),
        PathBuf::from,
    )
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::{Command, DEFAULT_RUNS, parse};
    use crate::campaign::Options;
    use crate::target;

    fn parse_args(args: &[&str]) -> Option<Command> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        parse(&args)
    }

    fn fuzz_folder(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
    }

    #[test]
    fn reads_a_campaign_with_its_defaults() {
        assert_eq!(
            parse_args(&["fixture-panic"]),
            Some(Command::Fuzz(Options {
                target: target::find("fixture-panic").unwrap(),
                seed: 0,
                runs: DEFAULT_RUNS,
                time_limit: Duration::from_millis(1000),
                corpus: fuzz_folder("corpus"),
                regressions: fuzz_folder("regressions"),
            }))
        );
    }

    #[test]
    fn reads_a_campaign_with_every_flag_in_any_order() {
        assert_eq!(
            parse_args(&[
                "fixture-hang",
                "--regressions",
                "saved",
                "--runs",
                "5",
                "--corpus",
                "inputs",
                "--time-limit",
                "250",
                "--seed",
                "18446744073709551615",
            ]),
            Some(Command::Fuzz(Options {
                target: target::find("fixture-hang").unwrap(),
                seed: u64::MAX,
                runs: 5,
                time_limit: Duration::from_millis(250),
                corpus: PathBuf::from("inputs"),
                regressions: PathBuf::from("saved"),
            }))
        );
    }

    #[test]
    fn reads_a_replay_of_one_input() {
        assert_eq!(
            parse_args(&["fixture-panic", "--replay", "input.bin"]),
            Some(Command::Replay {
                target: target::find("fixture-panic").unwrap(),
                path: PathBuf::from("input.bin"),
            })
        );
    }

    #[test]
    fn reads_a_replay_of_every_saved_input() {
        assert_eq!(
            parse_args(&["--replay-saved"]),
            Some(Command::ReplaySaved {
                regressions: fuzz_folder("regressions"),
                time_limit: Duration::from_millis(1000),
            })
        );
        assert_eq!(
            parse_args(&[
                "--replay-saved",
                "--time-limit",
                "10",
                "--regressions",
                "saved"
            ]),
            Some(Command::ReplaySaved {
                regressions: PathBuf::from("saved"),
                time_limit: Duration::from_millis(10),
            })
        );
    }

    #[test]
    fn reads_a_child_of_a_campaign() {
        assert_eq!(
            parse_args(&["--child", "fixture-hang"]),
            Some(Command::Child(target::find("fixture-hang").unwrap()))
        );
    }

    #[test]
    fn rejects_what_it_does_not_know() {
        let rejected: &[&[&str]] = &[
            &[],
            &["parser"],
            &["parser", "--replay", "input.bin"],
            &["--child", "parser"],
            &["--child"],
            &["fixture-panic", "--jobs", "4"],
            &["fixture-panic", "--seed"],
            &["fixture-panic", "--seed", "1", "--seed", "2"],
            &["fixture-panic", "--seed", "-1"],
            &["fixture-panic", "--runs", "many"],
            &["fixture-panic", "--time-limit", "1.5"],
            &["fixture-panic", "--replay"],
            &["--replay-saved", "--seed", "1"],
            &["--replay-saved", "--time-limit", "soon"],
        ];
        for args in rejected {
            assert_eq!(parse_args(args), None, "{args:?}");
        }
    }
}
