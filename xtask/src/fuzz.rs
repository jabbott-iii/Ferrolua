//! `cargo xtask fuzz`: builds the fuzzer, the `lunaferra-fuzz` program in `fuzz/`, and runs it with
//! the arguments after `fuzz`. The fuzzer is a program of its own because it links the crates it
//! targets, which `xtask` doesn't depend on. It prints its own report and usage, and `cargo xtask`
//! exits with the fuzzer's status.

use std::env;
use std::ffi::OsString;
use std::process::Command;

use crate::error::Error;

/// Runs the fuzzer with `args`, and returns the status that `cargo run` exited with: the fuzzer's,
/// or Cargo's own when the build fails. It returns 1 for a status that doesn't fit in a byte, and
/// when a signal stopped the process on Unix, which leaves no status.
///
/// It runs Cargo from `CARGO`, which Cargo sets when it runs `xtask`, and finds it on the `PATH`
/// when the variable isn't set. The fuzzer and Cargo write straight to the terminal.
///
/// # Errors
///
/// Returns [`Error::Start`] when Cargo can't start.
pub(crate) fn run(args: &[OsString]) -> Result<u8, Error> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    Command::new(cargo)
        .args(["run", "--locked", "--package", "lunaferra-fuzz", "--"])
        .args(args)
        .status()
        .map(|status| {
            status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1)
        })
        .map_err(|source| Error::Start {
            program: "cargo run",
            source,
        })
}
