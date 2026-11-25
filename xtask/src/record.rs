//! `cargo xtask record`: runs PUC-Rio Lua 5.5.1 on the reference scripts, the Lua 5.5 twins of
//! the conformance scripts in `tests/conformance/reference/`, and saves what each one prints to a
//! `.out` file beside it, as the [quality document] describes. The conformance runner (M2.14) will
//! compare what each Ferrolua script prints with its twin's `.out` file.
//!
//! The steps:
//!
//! 1. [`Workspace::locate`] finds the workspace's root, and with it the folder of the reference
//!    scripts.
//! 2. It picks the scripts to record: every `.lua` file in that folder, sorted by name, or the one
//!    the command line names, which must be one of them.
//! 3. It runs `lua -E -v`, and refuses the interpreter unless it exits with status 0 and the first
//!    line it prints is `Lua 5.5.1`, alone or followed by whitespace, as PUC-Rio Lua 5.5.1's
//!    `Lua 5.5.1  Copyright (C) 1994-2026 Lua.org, PUC-Rio` is.
//! 4. It runs `lua -E <name>` on each script, from the scripts' folder, so that Lua names the chunk
//!    after the file alone, as the conformance runner will. `-E` makes Lua ignore `LUA_INIT` and the
//!    other environment variables that could change what a script prints, and the script reads an
//!    empty standard input.
//! 5. Only when every script ran and exited with status 0 does it save the `.out` files. A file
//!    that already holds what the script printed is left as it is.
//!
//! The interpreter is the program that `FERROLUA_PUC_LUA` names, or `lua` on the `PATH`. Its
//! standard error goes straight to the terminal, so a script's error message appears above the
//! error that stops the task.
//!
//! [quality document]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/04-quality.md#xtask

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::cargo::Workspace;
use crate::error::Error;

/// The folder of the reference scripts, from the workspace's root.
const REFERENCE: &str = "tests/conformance/reference";

/// Records the reference scripts of the workspace that holds the current folder: every one, or
/// only `script` when the command line names one. It returns the report, one line for each `.out`
/// file, which says whether the file is new, changed, or unchanged.
///
/// It runs Cargo from `CARGO`, which Cargo sets when it runs `xtask`, and Lua from
/// `FERROLUA_PUC_LUA`, each found on the `PATH`, as `cargo` and `lua`, when the variable isn't set.
///
/// # Errors
///
/// Returns an error, and saves no `.out` file, when the workspace can't be located
/// ([`Error::Start`], [`Error::Failed`], or [`Error::Output`] from `cargo metadata`), when the
/// scripts' folder can't be read or holds no script ([`Error::Read`] or [`Error::NoScripts`]),
/// when `script` isn't one of the scripts ([`Error::Read`] or [`Error::NotReference`]), when Lua
/// can't start or isn't Lua 5.5.1 ([`Error::NoLua`] or [`Error::WrongLua`]), or when Lua fails on
/// a script ([`Error::LuaFailed`]). Returns [`Error::Save`] when a `.out` file can't be saved, after
/// saving the ones before it.
pub(crate) fn run(script: Option<&Path>) -> Result<String, Error> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let lua = env::var_os("FERROLUA_PUC_LUA").unwrap_or_else(|| OsString::from("lua"));
    let folder = Workspace::locate(&cargo)?.root.join(REFERENCE);
    let names = select(&folder, script)?;
    check_version(&lua, &folder)?;
    let outputs = names
        .iter()
        .map(|name| record(&lua, &folder, name))
        .collect::<Result<Vec<_>, _>>()?;
    let mut report = String::new();
    for (name, output) in names.iter().zip(outputs) {
        let out = Path::new(name).with_extension("out");
        let path = folder.join(&out);
        let change = save(&path, &output).map_err(|source| Error::Save { path, source })?;
        report.push_str(&format!("{REFERENCE}/{}: {change}\n", out.display()));
    }
    Ok(report)
}

/// The file names of the scripts to record, from `folder`: every script, or only `script` when the
/// command line names one.
fn select(folder: &Path, script: Option<&Path>) -> Result<Vec<OsString>, Error> {
    let mut names = scripts_in(folder).map_err(|source| Error::Read {
        path: folder.to_owned(),
        source,
    })?;
    match script {
        Some(script) => {
            let wanted = fs::canonicalize(script).map_err(|source| Error::Read {
                path: script.to_owned(),
                source,
            })?;
            names.retain(|name| {
                fs::canonicalize(folder.join(name)).is_ok_and(|path| path == wanted)
            });
            if names.is_empty() {
                Err(Error::NotReference {
                    path: script.to_owned(),
                })
            } else {
                Ok(names)
            }
        }
        None if names.is_empty() => Err(Error::NoScripts {
            folder: folder.to_owned(),
        }),
        None => Ok(names),
    }
}

/// The file names of the `.lua` files in `folder`, sorted by name.
fn scripts_in(folder: &Path) -> io::Result<Vec<OsString>> {
    fs::read_dir(folder)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()
        .map(|mut names| {
            names.retain(|name| Path::new(name).extension() == Some(OsStr::new("lua")));
            names.sort();
            names
        })
}

/// Checks that `lua` is Lua 5.5.1, from the first line that `lua -E -v` prints.
fn check_version(lua: &OsStr, folder: &Path) -> Result<(), Error> {
    let output = run_lua(lua, OsStr::new("-v"), folder)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = stdout.lines().next().unwrap_or("");
    if output.status.success() && is_lua_5_5_1(reported) {
        Ok(())
    } else {
        Err(Error::WrongLua {
            lua: PathBuf::from(lua),
            reported: reported.to_owned(),
        })
    }
}

/// Whether `line`, the first line that `lua -v` printed, names Lua 5.5.1: it is `Lua 5.5.1`,
/// either alone or followed by whitespace and more text.
fn is_lua_5_5_1(line: &str) -> bool {
    line.strip_prefix("Lua 5.5.1")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// Runs `lua` on the script `name` in `folder`, and returns what it printed to standard output.
fn record(lua: &OsStr, folder: &Path, name: &OsStr) -> Result<Vec<u8>, Error> {
    run_lua(lua, name, folder).and_then(|output| {
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(Error::LuaFailed {
                lua: PathBuf::from(lua),
                script: folder.join(name),
                status: output.status,
            })
        }
    })
}

/// Runs `lua -E <arg>` in `folder`, with an empty standard input, and its standard error on the
/// terminal.
fn run_lua(lua: &OsStr, arg: &OsStr, folder: &Path) -> Result<Output, Error> {
    Command::new(lua)
        .arg("-E")
        .arg(arg)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|source| Error::NoLua {
            lua: PathBuf::from(lua),
            source,
        })
}

/// How saving a `.out` file changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// The file didn't exist.
    New,
    /// The file held other output, which the new output replaced.
    Changed,
    /// The file already held the output, so it wasn't written.
    Unchanged,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Change::New => "new",
            Change::Changed => "changed",
            Change::Unchanged => "unchanged",
        })
    }
}

/// Writes `output` to the file at `path`, unless the file holds it already.
fn save(path: &Path, output: &[u8]) -> io::Result<Change> {
    let change = match fs::read(path) {
        Ok(old) if old == output => return Ok(Change::Unchanged),
        Ok(_) => Change::Changed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Change::New,
        Err(error) => return Err(error),
    };
    fs::write(path, output).map(|()| change)
}

#[cfg(test)]
mod tests {
    use super::is_lua_5_5_1;

    #[test]
    fn accepts_the_version_line_of_puc_rio_lua_5_5_1() {
        assert!(is_lua_5_5_1(
            "Lua 5.5.1  Copyright (C) 1994-2026 Lua.org, PUC-Rio"
        ));
    }

    #[test]
    fn accepts_lua_5_5_1_alone() {
        assert!(is_lua_5_5_1("Lua 5.5.1"));
    }

    #[test]
    fn refuses_another_release_of_lua_5_5() {
        assert!(!is_lua_5_5_1(
            "Lua 5.5.0  Copyright (C) 1994-2025 Lua.org, PUC-Rio"
        ));
    }

    #[test]
    fn refuses_a_version_that_only_starts_with_5_5_1() {
        assert!(!is_lua_5_5_1(
            "Lua 5.5.10  Copyright (C) 1994-2027 Lua.org, PUC-Rio"
        ));
    }

    #[test]
    fn refuses_luajit() {
        assert!(!is_lua_5_5_1(
            "LuaJIT 2.1.1744318430 -- Copyright (C) 2005-2025 Mike Pall. https://luajit.org/"
        ));
    }

    #[test]
    fn refuses_an_empty_line() {
        assert!(!is_lua_5_5_1(""));
    }
}
