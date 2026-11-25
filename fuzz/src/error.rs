//! The errors that stop the fuzzer before it can report, each with the message it prints for it.
//!
//! An input that makes a target fail isn't an error: the fuzzer reports it as a finding.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Why the fuzzer stopped before it could report.
#[derive(Debug)]
pub(crate) enum Error {
    /// The fuzzer couldn't find the path of its own program, which it runs its child processes
    /// from.
    CurrentExe(io::Error),
    /// The fuzzer couldn't make, write, or read the files in its work folder, through which it
    /// hands a child process its inputs and watches its progress.
    Work(io::Error),
    /// A child process couldn't start.
    Spawn(io::Error),
    /// A target's folder of corpus inputs couldn't be read.
    Corpus {
        /// The target's corpus folder.
        path: PathBuf,
        /// Why it couldn't be read.
        source: io::Error,
    },
    /// A failing input couldn't be saved.
    Save {
        /// The folder it should be saved in.
        path: PathBuf,
        /// Why it couldn't be saved.
        source: io::Error,
    },
    /// The saved inputs couldn't be read.
    Saved {
        /// The folder of saved inputs, or of one target's saved inputs.
        path: PathBuf,
        /// Why it couldn't be read.
        source: io::Error,
    },
    /// A folder of saved inputs is named after no target.
    UnknownFolder {
        /// The folder.
        path: PathBuf,
    },
    /// The input to replay couldn't be read.
    Replay {
        /// The input's file.
        path: PathBuf,
        /// Why it couldn't be read.
        source: io::Error,
    },
    /// A child process was handed a batch of inputs that is cut short.
    Batch,
    /// A child process couldn't read its batch or write its progress.
    Child(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CurrentExe(source) => {
                write!(
                    f,
                    "couldn't find the path of the fuzzer's own program: {source}"
                )
            }
            Error::Work(source) => write!(f, "couldn't use the fuzzer's work folder: {source}"),
            Error::Spawn(source) => write!(f, "couldn't start a child process: {source}"),
            Error::Corpus { path, source } => {
                write!(
                    f,
                    "couldn't read the corpus in `{}`: {source}",
                    path.display()
                )
            }
            Error::Save { path, source } => {
                write!(
                    f,
                    "couldn't save the input in `{}`: {source}",
                    path.display()
                )
            }
            Error::Saved { path, source } => {
                write!(
                    f,
                    "couldn't read the saved inputs in `{}`: {source}",
                    path.display()
                )
            }
            Error::UnknownFolder { path } => {
                write!(f, "`{}` is named after no fuzz target", path.display())
            }
            Error::Replay { path, source } => {
                write!(f, "couldn't read the input `{}`: {source}", path.display())
            }
            Error::Batch => write!(f, "the batch of inputs on standard input is cut short"),
            Error::Child(source) => {
                write!(f, "couldn't read the batch or write the progress: {source}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::CurrentExe(source)
            | Error::Work(source)
            | Error::Spawn(source)
            | Error::Child(source)
            | Error::Corpus { source, .. }
            | Error::Save { source, .. }
            | Error::Saved { source, .. }
            | Error::Replay { source, .. } => Some(source),
            Error::UnknownFolder { .. } | Error::Batch => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::io;
    use std::path::PathBuf;

    use super::Error;

    fn denied() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, "denied")
    }

    #[test]
    fn says_what_failed_and_why() {
        let path = || PathBuf::from("fuzz/x");
        let cases = [
            (
                Error::CurrentExe(denied()),
                "couldn't find the path of the fuzzer's own program: denied",
            ),
            (
                Error::Work(denied()),
                "couldn't use the fuzzer's work folder: denied",
            ),
            (
                Error::Spawn(denied()),
                "couldn't start a child process: denied",
            ),
            (
                Error::Corpus {
                    path: path(),
                    source: denied(),
                },
                "couldn't read the corpus in `fuzz/x`: denied",
            ),
            (
                Error::Save {
                    path: path(),
                    source: denied(),
                },
                "couldn't save the input in `fuzz/x`: denied",
            ),
            (
                Error::Saved {
                    path: path(),
                    source: denied(),
                },
                "couldn't read the saved inputs in `fuzz/x`: denied",
            ),
            (
                Error::UnknownFolder { path: path() },
                "`fuzz/x` is named after no fuzz target",
            ),
            (
                Error::Replay {
                    path: path(),
                    source: denied(),
                },
                "couldn't read the input `fuzz/x`: denied",
            ),
            (
                Error::Batch,
                "the batch of inputs on standard input is cut short",
            ),
            (
                Error::Child(denied()),
                "couldn't read the batch or write the progress: denied",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn keeps_the_underlying_error_as_its_source() {
        let error = Error::Save {
            path: PathBuf::from("fuzz/x"),
            source: denied(),
        };
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("denied".to_owned())
        );
        assert!(Error::Batch.source().is_none());
        assert!(
            Error::UnknownFolder {
                path: PathBuf::new()
            }
            .source()
            .is_none()
        );
    }
}
