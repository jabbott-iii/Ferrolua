//! Runs the programs that the tasks drive, such as Cargo and the LLVM tools, and turns a program
//! that couldn't start, or that failed, into an [`Error`].

use std::process::Command;

use crate::error::Error;

/// Runs `command` to completion and returns what it wrote to its standard output.
///
/// `program` names the command in an error. Its standard error is captured and dropped unless the
/// caller sets it, so each caller that runs a program for the user sets it to
/// [`Stdio::inherit`](std::process::Stdio::inherit), and the program's own messages reach the
/// terminal.
///
/// # Errors
///
/// Returns [`Error::Start`] when the program can't start, and [`Error::Failed`] when it exits with
/// a status other than success.
pub(crate) fn capture(program: &'static str, command: &mut Command) -> Result<Vec<u8>, Error> {
    let output = command
        .output()
        .map_err(|source| Error::Start { program, source })?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(Error::Failed {
            program,
            status: output.status,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::io;
    use std::process::{Command, Stdio};

    use super::capture;
    use crate::error::Error;

    #[test]
    fn returns_what_the_program_printed() {
        let expected = Command::new("rustc").arg("--version").output().unwrap();
        let output = capture("rustc", Command::new("rustc").arg("--version")).unwrap();
        assert_eq!(output, expected.stdout);
    }

    #[test]
    fn fails_when_the_program_cannot_start() {
        let missing = env::temp_dir().join("ferrolua-xtask-no-such-program");
        let error = capture("missing", &mut Command::new(missing)).unwrap_err();
        assert!(matches!(
            error,
            Error::Start { program: "missing", source } if source.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn fails_when_the_program_exits_with_a_failure_status() {
        let error = capture(
            "rustc",
            Command::new("rustc")
                .arg("--no-such-option")
                .stderr(Stdio::null()),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            Error::Failed { program: "rustc", status } if status.code() == Some(1)
        ));
    }
}
