//! The errors that stop a task, each with the message that `cargo xtask` prints for it.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;

/// Why a task stopped before it could report.
#[derive(Debug)]
pub(crate) enum Error {
    /// A program couldn't start, for example because it isn't installed.
    Start {
        /// The program, as the message names it.
        program: &'static str,
        /// Why it couldn't start.
        source: io::Error,
    },
    /// A program ran and exited with a status that means it failed.
    Failed {
        /// The program, as the message names it.
        program: &'static str,
        /// The status it exited with.
        status: ExitStatus,
    },
    /// A program's output isn't in the form the task reads.
    Output {
        /// The program, as the message names it.
        program: &'static str,
        /// What is wrong with the output.
        problem: String,
    },
    /// A program from the `llvm-tools` component of the toolchain isn't installed.
    MissingTool {
        /// Where the program should be.
        path: PathBuf,
    },
    /// A folder couldn't be removed.
    Remove {
        /// The folder.
        path: PathBuf,
        /// Why it couldn't be removed.
        source: io::Error,
    },
    /// Cargo built no test programs, so nothing measures the workspace's code.
    NoTests,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Start { program, source } => write!(f, "couldn't start `{program}`: {source}"),
            Error::Failed { program, status } => write!(f, "`{program}` failed: {status}"),
            Error::Output { program, problem } => {
                write!(f, "unexpected output from `{program}`: {problem}")
            }
            Error::MissingTool { path } => write!(
                f,
                "`{}` doesn't exist: install it with `rustup component add llvm-tools`",
                path.display()
            ),
            Error::Remove { path, source } => {
                write!(f, "couldn't remove `{}`: {source}", path.display())
            }
            Error::NoTests => write!(f, "the workspace has no tests to measure"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;
    use std::process::Command;

    use super::Error;

    #[test]
    fn says_which_program_could_not_start_and_why() {
        let error = Error::Start {
            program: "rustc",
            source: io::Error::new(io::ErrorKind::NotFound, "not found"),
        };
        assert_eq!(error.to_string(), "couldn't start `rustc`: not found");
    }

    #[test]
    fn says_which_program_failed_and_its_status() {
        let status = Command::new("rustc")
            .arg("--no-such-option")
            .output()
            .unwrap()
            .status;
        let error = Error::Failed {
            program: "cargo test",
            status,
        };
        assert_eq!(error.to_string(), format!("`cargo test` failed: {status}"));
    }

    #[test]
    fn says_which_program_gave_unexpected_output() {
        let error = Error::Output {
            program: "cargo metadata",
            problem: "expected a value at byte 0".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "unexpected output from `cargo metadata`: expected a value at byte 0"
        );
    }

    #[test]
    fn says_how_to_install_a_missing_tool() {
        let error = Error::MissingTool {
            path: PathBuf::from("bin/llvm-cov"),
        };
        assert_eq!(
            error.to_string(),
            "`bin/llvm-cov` doesn't exist: install it with `rustup component add llvm-tools`"
        );
    }

    #[test]
    fn says_which_folder_could_not_be_removed_and_why() {
        let error = Error::Remove {
            path: PathBuf::from("target/coverage/profiles"),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        };
        assert_eq!(
            error.to_string(),
            "couldn't remove `target/coverage/profiles`: denied"
        );
    }

    #[test]
    fn says_there_are_no_tests() {
        assert_eq!(
            Error::NoTests.to_string(),
            "the workspace has no tests to measure"
        );
    }
}
