//! `cargo xtask coverage`: builds the workspace's tests with coverage instrumentation, runs them,
//! and reports which lines and regions of each crate they ran. The report passes only when they
//! ran every one.
//!
//! The steps, each in a module of its own:
//!
//! 1. [`Llvm::locate`] finds `llvm-profdata` and `llvm-cov` in the toolchain's sysroot.
//! 2. [`Workspace::locate`] asks `cargo metadata` where the workspace and its crates are.
//! 3. [`TestRun::run`] builds the tests with `-C instrument-coverage` and runs them, and each
//!    process writes a profile of the counts it ran.
//! 4. [`Llvm::measure`] merges the profiles and reads the coverage back from `llvm-cov export`.
//! 5. [`Report::new`] counts each crate's lines and regions.

use std::env;
use std::ffi::OsString;

use crate::cargo::{TestRun, Workspace};
use crate::error::Error;
use crate::llvm::Llvm;
use crate::report::Report;

/// Measures the coverage of the workspace that holds the current folder.
///
/// It runs Cargo from `CARGO`, which Cargo sets when it runs `xtask`, and `rustc` from `RUSTC`,
/// each found on the `PATH` when the variable isn't set.
///
/// # Errors
///
/// Returns the error of the first step that fails.
pub(crate) fn run() -> Result<Report, Error> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let llvm = Llvm::locate(&rustc)?;
    let workspace = Workspace::locate(&cargo)?;
    let tests = TestRun::run(&cargo, &workspace)?;
    llvm.measure(&tests)
        .map(|coverage| Report::new(&workspace, &coverage))
}
