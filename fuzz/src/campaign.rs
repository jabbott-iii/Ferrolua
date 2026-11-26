//! A campaign: `cargo xtask fuzz <target>` runs the target on one mutated input after another, in
//! batches of [`BATCH`] runs to a child process, and stops at the first input that fails. It then
//! shrinks that input, saves it, and reports it. Without a failure, it stops once it has made the
//! runs that `--runs` asks for, or once the time that `--seconds` gives is up. It checks the time
//! before each batch, so a campaign can run past its time by up to one batch.
//!
//! A campaign stops at the first failure, because the inputs after it would most often find the
//! same bug again. Once the bug is fixed, the next campaign from the same seed runs past it.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::Error;
use crate::folder;
use crate::mutate::mutate;
use crate::runner::{Failure, Runner, Verdict};
use crate::shrink::shrink;
use crate::target::Target;

/// How many runs go to one child process.
pub(crate) const BATCH: u64 = 1000;

/// How many edits shrinking may try, which bounds how long it takes.
pub(crate) const SHRINK_LIMIT: u32 = 10_000;

/// The settings of a campaign, which `cargo xtask fuzz` reads from its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    /// The target to run.
    pub(crate) target: &'static Target,
    /// The seed of the first run. Run `n` uses the seed `seed + n`.
    pub(crate) seed: u64,
    /// How many runs to make, unless one fails first.
    pub(crate) runs: u64,
    /// How long the campaign may start new batches of runs, or `None` for no limit.
    pub(crate) duration: Option<Duration>,
    /// How long one input may run before it counts as a hang.
    pub(crate) time_limit: Duration,
    /// The folder of corpus folders, one for each target.
    pub(crate) corpus: PathBuf,
    /// The folder of saved inputs, with one folder for each target.
    pub(crate) regressions: PathBuf,
}

/// What a campaign found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Report {
    /// No run failed.
    Passed {
        /// The target's name.
        target: &'static str,
        /// How many runs it made.
        runs: u64,
        /// The seed of its first run.
        seed: u64,
    },
    /// A run failed, and its input was shrunk and saved.
    Found(Found),
}

/// A run that failed, and where its shrunk input is saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    /// The target's name.
    pub(crate) target: &'static str,
    /// The run's number, counting from 0.
    pub(crate) run: u64,
    /// The run's seed, which makes its input again.
    pub(crate) seed: u64,
    /// How the shrunk input fails.
    pub(crate) failure: Failure,
    /// The length of the run's input.
    pub(crate) length: usize,
    /// The length of the shrunk input.
    pub(crate) shrunk: usize,
    /// Where the shrunk input is saved.
    pub(crate) path: PathBuf,
}

impl Report {
    /// The status `cargo xtask fuzz` exits with: 0 when no run failed, and 1 when one did.
    pub(crate) fn status(&self) -> u8 {
        match self {
            Report::Passed { .. } => 0,
            Report::Found(_) => 1,
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Report::Passed { target, runs, seed } => {
                writeln!(
                    f,
                    "{target}: no run failed (runs: {runs}, first seed: {seed})"
                )
            }
            Report::Found(Found {
                target,
                run,
                seed,
                failure,
                length,
                shrunk,
                path,
            }) => {
                let path = path.display();
                writeln!(
                    f,
                    "{target}: run {run} (seed {seed}) {failure}\n  \
                     length: {length}, shrunk to {shrunk}\n  \
                     saved: {path}\n  \
                     replay: cargo xtask fuzz {target} --replay {path}"
                )
            }
        }
    }
}

/// Runs a campaign, running each batch in a child process that runs `exe`, with its work files in
/// `work`.
///
/// # Errors
///
/// Returns an error when the corpus can't be read, when a child process can't run, or when the
/// failing input can't be saved.
pub(crate) fn run(exe: &Path, work: PathBuf, options: &Options) -> Result<Report, Error> {
    let target = options.target;
    let corpus = folder::corpus(&options.corpus.join(target.name))?;
    let runner = Runner::new(exe, options.time_limit, work)?;
    let began = Instant::now();
    let mut start = 0;
    while start < options.runs
        && options
            .duration
            .is_none_or(|duration| began.elapsed() < duration)
    {
        let end = options.runs.min(start.saturating_add(BATCH));
        let mut inputs: Vec<Vec<u8>> = (start..end)
            .map(|run| mutate(&corpus, options.seed.wrapping_add(run)))
            .collect();
        if let Verdict::Failed { index, failure } = runner.run(target, &inputs)? {
            let run = start + u64::try_from(index).expect("a position in a batch fits in 64 bits");
            return found(&runner, options, run, inputs.swap_remove(index), failure);
        }
        start = end;
    }
    Ok(Report::Passed {
        target: target.name,
        runs: start,
        seed: options.seed,
    })
}

/// Shrinks the input of run `run`, which failed as `failure` says, then saves it and reports it.
fn found(
    runner: &Runner,
    options: &Options,
    run: u64,
    input: Vec<u8>,
    failure: Failure,
) -> Result<Report, Error> {
    let target = options.target;
    let length = input.len();
    shrink(input, failure, SHRINK_LIMIT, |candidate| {
        runner
            .run(target, &[candidate.to_vec()])
            .map(Verdict::into_failure)
    })
    .and_then(|shrunk| {
        folder::save(&options.regressions, target.name, &shrunk.input).map(|path| {
            Report::Found(Found {
                target: target.name,
                run,
                seed: options.seed.wrapping_add(run),
                failure: shrunk.failure,
                length,
                shrunk: shrunk.input.len(),
                path,
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::{Found, Options, Report, run};
    use crate::error::Error;
    use crate::runner::Failure;
    use crate::target;

    /// An empty folder of this test's own under the system's temporary folder.
    fn folder(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!(
            "lunaferra-fuzz-campaign-{}-{name}",
            std::process::id()
        ));
        // The folder is left from an earlier run only if that run stopped early.
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn options(folder: &Path) -> Options {
        Options {
            target: target::find("fixture-panic").unwrap(),
            seed: 0,
            runs: 10,
            duration: None,
            time_limit: Duration::from_millis(1000),
            corpus: folder.join("corpus"),
            regressions: folder.join("regressions"),
        }
    }

    #[test]
    fn fails_when_its_work_folder_cannot_be_made() {
        let folder = folder("no-work");
        fs::write(folder.join("work"), "a file where the folder goes").unwrap();
        let error = run(
            &folder.join("lunaferra-fuzz"),
            folder.join("work").join("inner"),
            &options(&folder),
        )
        .unwrap_err();
        assert!(matches!(error, Error::Work(_)));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_a_child_cannot_start() {
        let folder = folder("no-child");
        let error = run(
            &folder.join("no-such-program"),
            folder.join("work"),
            &options(&folder),
        )
        .unwrap_err();
        assert!(matches!(error, Error::Spawn(_)));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn makes_no_run_when_asked_for_none() {
        let folder = folder("no-runs");
        let options = Options {
            runs: 0,
            ..options(&folder)
        };
        let report = run(
            &folder.join("no-such-program"),
            folder.join("work"),
            &options,
        )
        .unwrap();
        assert_eq!(
            report,
            Report::Passed {
                target: "fixture-panic",
                runs: 0,
                seed: 0
            }
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn makes_no_run_once_its_time_is_up() {
        let folder = folder("no-time");
        let options = Options {
            duration: Some(Duration::ZERO),
            ..options(&folder)
        };
        let report = run(
            &folder.join("no-such-program"),
            folder.join("work"),
            &options,
        )
        .unwrap();
        assert_eq!(
            report,
            Report::Passed {
                target: "fixture-panic",
                runs: 0,
                seed: 0
            }
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn reports_a_pass_with_status_0() {
        let report = Report::Passed {
            target: "fixture-hang",
            runs: 2500,
            seed: 7,
        };
        assert_eq!(
            report.to_string(),
            "fixture-hang: no run failed (runs: 2500, first seed: 7)\n"
        );
        assert_eq!(report.status(), 0);
    }

    #[test]
    fn reports_a_failure_with_its_saved_input_and_status_1() {
        let path = PathBuf::from("fuzz/regressions/fixture-hang/af63bd4c8601b7df.bin");
        let report = Report::Found(Found {
            target: "fixture-hang",
            run: 12,
            seed: 19,
            failure: Failure::Hang(Duration::from_millis(1000)),
            length: 5,
            shrunk: 1,
            path: path.clone(),
        });
        assert_eq!(
            report.to_string(),
            format!(
                "fixture-hang: run 12 (seed 19) ran longer than the time limit of 1000 ms\n  \
                 length: 5, shrunk to 1\n  \
                 saved: {0}\n  \
                 replay: cargo xtask fuzz fixture-hang --replay {0}\n",
                path.display()
            )
        );
        assert_eq!(report.status(), 1);
    }
}
