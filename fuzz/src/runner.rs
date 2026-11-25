//! The parent side of a run: it starts a child process on a batch of inputs, stops the child when
//! one input runs past the time limit, and says which input failed and how.
//!
//! The parent hands the child its batch, and takes back its progress, through two files in a work
//! folder of its own, so that no pipe can fill up and stall either process. The [`child`] module
//! describes what the child writes.
//!
//! [`child`]: crate::child

use std::fmt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::batch;
use crate::child;
use crate::error::Error;
use crate::target::{Kind, Target};

/// How long the parent waits between two looks at a child's progress.
const POLL: Duration = Duration::from_millis(1);

/// The message of each `expect` on waiting for or stopping a child. Each fails only for a process
/// that isn't this process's child or was already waited for, and the runner waits for each child
/// it starts exactly once.
const OWN_CHILD: &str = "waiting for or stopping a child that was never waited for can't fail";

/// Runs batches of inputs in child processes, each under a time limit per input.
#[derive(Debug)]
pub(crate) struct Runner {
    /// The fuzzer's own program, which each child runs.
    exe: PathBuf,
    /// How long one input may run before the runner stops the child and reports a hang.
    time_limit: Duration,
    /// The folder of the batch and progress files.
    work: PathBuf,
}

/// How a batch ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Every input passed.
    Passed,
    /// An input failed, and the child ran none after it.
    Failed {
        /// The input's position in the batch, counting from 0.
        index: usize,
        /// How it failed.
        failure: Failure,
    },
}

impl Verdict {
    /// How the batch ended, without the details.
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Verdict::Passed => Kind::Passed,
            Verdict::Failed { failure, .. } => failure.kind(),
        }
    }

    /// The failure, if an input failed.
    pub(crate) fn into_failure(self) -> Option<Failure> {
        match self {
            Verdict::Passed => None,
            Verdict::Failed { failure, .. } => Some(failure),
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Passed => f.write_str("passed"),
            Verdict::Failed { failure, .. } => failure.fmt(f),
        }
    }
}

/// How an input failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The target panicked with this message.
    Panic(String),
    /// The input ran past this time limit.
    Hang(Duration),
    /// The child process stopped with this status, which isn't the status of a caught panic.
    Crash(ExitStatus),
}

impl Failure {
    /// How the input failed, without the details.
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Failure::Panic(_) => Kind::Panicked,
            Failure::Hang(_) => Kind::Hung,
            Failure::Crash(_) => Kind::Crashed,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Panic(message) => write!(f, "panicked: {message}"),
            Failure::Hang(limit) => write!(
                f,
                "ran longer than the time limit of {} ms",
                limit.as_millis()
            ),
            Failure::Crash(status) => write!(f, "crashed: {status}"),
        }
    }
}

/// How a child process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ended {
    /// It exited with this status.
    Exited(ExitStatus),
    /// The runner stopped it, because an input ran past the time limit.
    TimedOut,
}

impl Runner {
    /// Makes a runner whose children run `exe`, and makes its work folder, `work`, if it doesn't
    /// exist. Dropping the runner removes the folder.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Work`] when the folder can't be made.
    pub(crate) fn new(exe: &Path, time_limit: Duration, work: PathBuf) -> Result<Runner, Error> {
        fs::create_dir_all(&work)
            .map(|()| Runner {
                exe: exe.to_owned(),
                time_limit,
                work,
            })
            .map_err(Error::Work)
    }

    /// Runs `target` on each input of `batch`, in order, in one child process, and stops at the
    /// first input that fails. `batch` must hold at least one input.
    ///
    /// The time limit counts from when the child starts an input, and for the first input from
    /// when the child process starts.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Work`] when the batch or the progress file can't be written or read, and
    /// [`Error::Spawn`] when the child can't start.
    pub(crate) fn run(&self, target: &Target, batch: &[Vec<u8>]) -> Result<Verdict, Error> {
        let batch_file = self.work.join("batch");
        let progress_file = self.work.join("progress");
        let (stdin, stdout) = fs::write(&batch_file, batch::encode(batch))
            .and_then(|()| File::open(&batch_file))
            .and_then(|stdin| File::create(&progress_file).map(|stdout| (stdin, stdout)))
            .map_err(Error::Work)?;
        let mut child = Command::new(&self.exe)
            .args(["--child", target.name])
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(Error::Spawn)?;
        let ended = self.wait(&mut child, &progress_file);
        fs::read(&progress_file)
            .map(|progress| verdict(ended, &progress, batch.len(), self.time_limit))
            .map_err(Error::Work)
    }

    /// Waits for `child` to exit, and stops it once the input it is running has run past the time
    /// limit. The child has started a new input whenever its progress file grows.
    fn wait(&self, child: &mut Child, progress_file: &Path) -> Ended {
        let mut progress = 0;
        let mut since = Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect(OWN_CHILD) {
                return Ended::Exited(status);
            }
            // A progress file that can't be read counts as no progress. The read after the child
            // ends reports the error.
            let now = fs::metadata(progress_file).map_or(0, |metadata| metadata.len());
            if now != progress {
                progress = now;
                since = Instant::now();
            } else if since.elapsed() >= self.time_limit {
                child.kill().expect(OWN_CHILD);
                child.wait().expect(OWN_CHILD);
                return Ended::TimedOut;
            }
            thread::sleep(POLL);
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        // Removing the work folder is a cleanup: a folder left behind holds nothing but the last
        // batch and its progress, in the system's temporary folder.
        let _ = fs::remove_dir_all(&self.work);
    }
}

/// Reads how a batch of `length` inputs ended from how its child ended and the progress it wrote.
fn verdict(ended: Ended, progress: &[u8], length: usize, time_limit: Duration) -> Verdict {
    let started = progress.iter().take_while(|&&byte| byte == b'\n').count();
    // The last input the child started is the one that failed. A child that ended before it
    // started one failed on the first.
    let index = started.min(length).saturating_sub(1);
    let failure = match ended {
        Ended::Exited(status) if status.success() => return Verdict::Passed,
        Ended::Exited(status) if status.code() == Some(i32::from(child::PANICKED)) => {
            let rest = &progress[started..];
            Failure::Panic(
                String::from_utf8_lossy(rest.strip_prefix(b"!").unwrap_or(rest)).into_owned(),
            )
        }
        Ended::Exited(status) => Failure::Crash(status),
        Ended::TimedOut => Failure::Hang(time_limit),
    };
    Verdict::Failed { index, failure }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitStatus, Stdio};
    use std::time::Duration;

    use super::{Ended, Failure, Runner, Verdict, verdict};
    use crate::error::Error;
    use crate::target::{self, Kind};

    const LIMIT: Duration = Duration::from_millis(250);

    /// A folder of this test's own under the system's temporary folder, removed if it exists.
    fn work(name: &str) -> PathBuf {
        let work = env::temp_dir().join(format!(
            "lunaferra-fuzz-runner-{}-{name}",
            std::process::id()
        ));
        // The folder is left from an earlier run only if that run stopped early.
        let _ = fs::remove_dir_all(&work);
        work
    }

    /// The status of a program that exits with status 1.
    fn status_1() -> ExitStatus {
        Command::new("rustc")
            .arg("--no-such-option")
            .stderr(Stdio::null())
            .status()
            .unwrap()
    }

    /// The status of a program that succeeds.
    fn success() -> ExitStatus {
        Command::new("rustc")
            .arg("--version")
            .stdout(Stdio::null())
            .status()
            .unwrap()
    }

    #[test]
    fn makes_its_work_folder_and_removes_it_when_dropped() {
        let folder = work("made");
        let runner = Runner::new(Path::new("lunaferra-fuzz"), LIMIT, folder.clone()).unwrap();
        assert!(folder.is_dir());
        drop(runner);
        assert!(!folder.exists());
    }

    #[test]
    fn fails_when_its_work_folder_cannot_be_made() {
        let file = work("file-in-the-way");
        fs::write(&file, "a file where a folder goes\n").unwrap();
        let error = Runner::new(Path::new("lunaferra-fuzz"), LIMIT, file.join("work")).unwrap_err();
        assert!(matches!(error, Error::Work(_)));
        fs::remove_file(file).unwrap();
    }

    #[test]
    fn fails_when_its_work_folder_is_gone() {
        let folder = work("gone");
        let runner = Runner::new(Path::new("lunaferra-fuzz"), LIMIT, folder.clone()).unwrap();
        fs::remove_dir_all(&folder).unwrap();
        let error = runner
            .run(target::find("fixture-panic").unwrap(), &[Vec::new()])
            .unwrap_err();
        assert!(matches!(error, Error::Work(source) if source.kind() == io::ErrorKind::NotFound));
    }

    #[test]
    fn fails_when_the_child_cannot_start() {
        let folder = work("no-program");
        let runner = Runner::new(&folder.join("no-such-program"), LIMIT, folder.clone()).unwrap();
        let error = runner
            .run(target::find("fixture-panic").unwrap(), &[Vec::new()])
            .unwrap_err();
        assert!(matches!(error, Error::Spawn(source) if source.kind() == io::ErrorKind::NotFound));
    }

    #[test]
    fn passes_a_batch_whose_child_succeeds() {
        assert_eq!(
            verdict(Ended::Exited(success()), b"\n\n", 2, LIMIT),
            Verdict::Passed
        );
    }

    #[test]
    fn blames_a_crash_on_the_last_input_started() {
        let status = status_1();
        assert_eq!(
            verdict(Ended::Exited(status), b"\n\n", 3, LIMIT),
            Verdict::Failed {
                index: 1,
                failure: Failure::Crash(status)
            }
        );
    }

    #[test]
    fn blames_the_first_input_when_none_started() {
        let status = status_1();
        assert_eq!(
            verdict(Ended::Exited(status), b"", 3, LIMIT),
            Verdict::Failed {
                index: 0,
                failure: Failure::Crash(status)
            }
        );
    }

    #[test]
    fn blames_a_hang_on_the_last_input_started() {
        assert_eq!(
            verdict(Ended::TimedOut, b"\n\n\n", 3, LIMIT),
            Verdict::Failed {
                index: 2,
                failure: Failure::Hang(LIMIT)
            }
        );
    }

    #[test]
    fn describes_each_failure() {
        let status = status_1();
        assert_eq!(
            Failure::Panic("oops".to_owned()).to_string(),
            "panicked: oops"
        );
        assert_eq!(
            Failure::Hang(LIMIT).to_string(),
            "ran longer than the time limit of 250 ms"
        );
        assert_eq!(
            Failure::Crash(status).to_string(),
            format!("crashed: {status}")
        );
    }

    #[test]
    fn describes_a_batch_by_how_it_ended() {
        assert_eq!(Verdict::Passed.to_string(), "passed");
        assert_eq!(
            Verdict::Failed {
                index: 3,
                failure: Failure::Hang(LIMIT)
            }
            .to_string(),
            "ran longer than the time limit of 250 ms"
        );
    }

    #[test]
    fn gives_the_kind_of_each_ending() {
        let kinds = [
            Verdict::Passed,
            Verdict::Failed {
                index: 0,
                failure: Failure::Panic(String::new()),
            },
            Verdict::Failed {
                index: 0,
                failure: Failure::Hang(LIMIT),
            },
            Verdict::Failed {
                index: 0,
                failure: Failure::Crash(status_1()),
            },
        ]
        .map(|verdict| verdict.kind());
        assert_eq!(
            kinds,
            [Kind::Passed, Kind::Panicked, Kind::Hung, Kind::Crashed]
        );
    }

    #[test]
    fn gives_the_failure_of_a_failed_batch_only() {
        assert_eq!(Verdict::Passed.into_failure(), None);
        assert_eq!(
            Verdict::Failed {
                index: 4,
                failure: Failure::Hang(LIMIT)
            }
            .into_failure(),
            Some(Failure::Hang(LIMIT))
        );
    }
}
