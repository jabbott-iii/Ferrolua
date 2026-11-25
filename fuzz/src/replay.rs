//! Replaying saved inputs. `cargo xtask fuzz <target> --replay <file>` runs one input in this
//! process, so a panic prints its message, its location, and, with `RUST_BACKTRACE=1`, its
//! backtrace. `cargo xtask fuzz --replay-saved` runs every saved input in a child process under the
//! time limit, and checks that each runs as its target's
//! [`replays_as`](crate::target::Target::replays_as) says. `cargo test` runs the second, so a bug
//! that a saved input once found can't come back unseen.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::Error;
use crate::folder;
use crate::runner::{Runner, Verdict};
use crate::target::{Kind, Target};

/// Runs `target` on the input in `path`, in this process. A panic in the target goes on past this
/// function.
///
/// # Errors
///
/// Returns [`Error::Replay`] when the file can't be read.
pub(crate) fn one(target: &Target, path: &Path) -> Result<(), Error> {
    fs::read(path)
        .map(|input| (target.run)(&input))
        .map_err(|source| Error::Replay {
            path: path.to_owned(),
            source,
        })
}

/// How each saved input ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replayed {
    inputs: Vec<Replay>,
}

/// How one saved input ran.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Replay {
    /// The input's target and file, such as `fixture-panic/c6d0a3857a0d9820.bin`.
    name: String,
    /// How it ran.
    verdict: Verdict,
    /// How it should have ended.
    expected: Kind,
}

impl Replayed {
    /// How many saved inputs ran as expected.
    fn as_expected(&self) -> usize {
        self.inputs
            .iter()
            .filter(|input| input.verdict.kind() == input.expected)
            .count()
    }

    /// The status `cargo xtask fuzz --replay-saved` exits with: 0 when every saved input ran as
    /// expected, and 1 otherwise.
    pub(crate) fn status(&self) -> u8 {
        if self.as_expected() == self.inputs.len() {
            0
        } else {
            1
        }
    }
}

impl fmt::Display for Replayed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = String::new();
        for Replay {
            name,
            verdict,
            expected,
        } in &self.inputs
        {
            text += &if verdict.kind() == *expected {
                format!("{name}: {verdict}\n")
            } else {
                format!("{name}: {verdict}, but it should have {expected}\n")
            };
        }
        text += &format!(
            "saved inputs: {}, as expected: {}\n",
            self.inputs.len(),
            self.as_expected()
        );
        f.write_str(&text)
    }
}

/// Runs every saved input in `regressions`, each in a child process that runs `exe` with its work
/// files in `work`, under `time_limit`.
///
/// # Errors
///
/// Returns an error when the saved inputs can't be read, or when a child process can't run.
pub(crate) fn saved(
    exe: &Path,
    work: PathBuf,
    regressions: &Path,
    time_limit: Duration,
) -> Result<Replayed, Error> {
    let saved = folder::saved(regressions)?;
    let runner = Runner::new(exe, time_limit, work)?;
    let mut inputs = Vec::with_capacity(saved.len());
    for input in saved {
        let verdict = runner.run(input.target, &[input.input])?;
        inputs.push(Replay {
            name: input.name,
            verdict,
            expected: input.target.replays_as,
        });
    }
    Ok(Replayed { inputs })
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{Replay, Replayed, one, saved};
    use crate::error::Error;
    use crate::folder;
    use crate::runner::{Failure, Verdict};
    use crate::target::{self, Kind};

    const LIMIT: Duration = Duration::from_millis(1000);

    /// An empty folder of this test's own under the system's temporary folder.
    fn folder(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!(
            "lunaferra-fuzz-replay-{}-{name}",
            std::process::id()
        ));
        // The folder is left from an earlier run only if that run stopped early.
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn replays_an_input_that_passes() {
        let folder = folder("one");
        let path = folder.join("input.bin");
        fs::write(&path, "FUZ").unwrap();
        one(target::find("fixture-panic").unwrap(), &path).unwrap();
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    #[should_panic(expected = "the input starts with FUZZ")]
    fn lets_the_panic_of_a_replayed_input_go_on() {
        /// Removes its folder when dropped, which happens while the panic unwinds the test.
        struct Removed(PathBuf);

        impl Drop for Removed {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        let folder = Removed(folder("one-panics"));
        let path = folder.0.join("input.bin");
        fs::write(&path, "FUZZ").unwrap();
        // The panic leaves `one` before it returns, so there is no result to look at.
        let _ = one(target::find("fixture-panic").unwrap(), &path);
    }

    #[test]
    fn fails_when_the_input_to_replay_cannot_be_read() {
        let folder = folder("one-missing");
        let path = folder.join("missing.bin");
        let error = one(target::find("fixture-panic").unwrap(), &path).unwrap_err();
        assert!(matches!(
            error,
            Error::Replay { path: missing, source } if missing == path && source.kind() == io::ErrorKind::NotFound
        ));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_the_saved_inputs_cannot_be_read() {
        let folder = folder("saved-unknown");
        fs::create_dir(folder.join("parser")).unwrap();
        let error = saved(
            &folder.join("no-such-program"),
            folder.join("work"),
            &folder,
            LIMIT,
        )
        .unwrap_err();
        assert!(matches!(error, Error::UnknownFolder { .. }));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_its_work_folder_cannot_be_made() {
        let folder = folder("saved-no-work");
        fs::write(folder.join("work"), "a file where the folder goes").unwrap();
        let regressions = folder.join("regressions");
        let error = saved(
            &folder.join("no-such-program"),
            folder.join("work").join("inner"),
            &regressions,
            LIMIT,
        )
        .unwrap_err();
        assert!(matches!(error, Error::Work(_)));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_a_child_cannot_start() {
        let folder = folder("saved-no-child");
        let regressions = folder.join("regressions");
        folder::save(&regressions, "fixture-panic", b"FUZZ").unwrap();
        let error = saved(
            &folder.join("no-such-program"),
            folder.join("work"),
            &regressions,
            LIMIT,
        )
        .unwrap_err();
        assert!(matches!(error, Error::Spawn(_)));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn reports_each_input_and_how_many_ran_as_expected() {
        let replayed = Replayed {
            inputs: vec![
                Replay {
                    name: "fixture-panic/a.bin".to_owned(),
                    verdict: Verdict::Failed {
                        index: 0,
                        failure: Failure::Panic("oops".to_owned()),
                    },
                    expected: Kind::Panicked,
                },
                Replay {
                    name: "fixture-panic/b.bin".to_owned(),
                    verdict: Verdict::Passed,
                    expected: Kind::Panicked,
                },
                Replay {
                    name: "parser/c.bin".to_owned(),
                    verdict: Verdict::Passed,
                    expected: Kind::Passed,
                },
            ],
        };
        assert_eq!(
            replayed.to_string(),
            "fixture-panic/a.bin: panicked: oops\n\
             fixture-panic/b.bin: passed, but it should have panicked\n\
             parser/c.bin: passed\n\
             saved inputs: 3, as expected: 2\n"
        );
        assert_eq!(replayed.status(), 1);
    }

    #[test]
    fn passes_when_there_is_no_saved_input() {
        let replayed = Replayed { inputs: Vec::new() };
        assert_eq!(replayed.to_string(), "saved inputs: 0, as expected: 0\n");
        assert_eq!(replayed.status(), 0);
    }
}
