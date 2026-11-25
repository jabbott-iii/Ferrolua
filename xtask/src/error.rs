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
    /// The Lua interpreter couldn't start, for example because it isn't installed.
    NoLua {
        /// The interpreter, as `FERROLUA_PUC_LUA` or the `PATH` gave it.
        lua: PathBuf,
        /// Why it couldn't start.
        source: io::Error,
    },
    /// The Lua interpreter failed when asked for its version, or reported a version other than
    /// Lua 5.5.1.
    WrongLua {
        /// The interpreter.
        lua: PathBuf,
        /// The first line it printed, which is empty when it printed nothing.
        reported: String,
    },
    /// The Lua interpreter exited with a failure status on a reference script.
    LuaFailed {
        /// The interpreter.
        lua: PathBuf,
        /// The script.
        script: PathBuf,
        /// The status it exited with.
        status: ExitStatus,
    },
    /// A file or folder couldn't be read.
    Read {
        /// The file or folder.
        path: PathBuf,
        /// Why it couldn't be read.
        source: io::Error,
    },
    /// A `.out` file couldn't be read or written.
    Save {
        /// The file.
        path: PathBuf,
        /// Why it couldn't be saved.
        source: io::Error,
    },
    /// The script named on the command line isn't one of the reference scripts.
    NotReference {
        /// The script, as the command line named it.
        path: PathBuf,
    },
    /// The folder of the reference scripts holds none.
    NoScripts {
        /// The folder.
        folder: PathBuf,
    },
}

/// How to fix an interpreter that `cargo xtask record` can't use.
const INSTALL_LUA: &str = "install PUC-Rio Lua 5.5.1, or set `FERROLUA_PUC_LUA` to its path";

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
            Error::NoLua { lua, source } => write!(
                f,
                "couldn't start `{}`: {source}: {INSTALL_LUA}",
                lua.display()
            ),
            Error::WrongLua { lua, reported } if reported.is_empty() => write!(
                f,
                "`{}` reports no version, not Lua 5.5.1: {INSTALL_LUA}",
                lua.display()
            ),
            Error::WrongLua { lua, reported } => write!(
                f,
                "`{}` reports `{reported}` as its version, not Lua 5.5.1: {INSTALL_LUA}",
                lua.display()
            ),
            Error::LuaFailed {
                lua,
                script,
                status,
            } => write!(
                f,
                "`{}` failed on `{}`: {status}",
                lua.display(),
                script.display()
            ),
            Error::Read { path, source } => {
                write!(f, "couldn't read `{}`: {source}", path.display())
            }
            Error::Save { path, source } => {
                write!(f, "couldn't save `{}`: {source}", path.display())
            }
            Error::NotReference { path } => write!(
                f,
                "`{}` isn't a reference script: name a `.lua` file in `tests/conformance/reference/`",
                path.display()
            ),
            Error::NoScripts { folder } => {
                write!(f, "`{}` holds no `.lua` scripts", folder.display())
            }
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

    #[test]
    fn says_which_lua_could_not_start_and_how_to_install_it() {
        let error = Error::NoLua {
            lua: PathBuf::from("lua"),
            source: io::Error::new(io::ErrorKind::NotFound, "not found"),
        };
        assert_eq!(
            error.to_string(),
            "couldn't start `lua`: not found: install PUC-Rio Lua 5.5.1, or set \
             `FERROLUA_PUC_LUA` to its path"
        );
    }

    #[test]
    fn says_which_version_the_wrong_lua_reports() {
        let error = Error::WrongLua {
            lua: PathBuf::from("/usr/bin/lua"),
            reported: "Lua 5.4.7  Copyright (C) 1994-2024 Lua.org, PUC-Rio".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "`/usr/bin/lua` reports `Lua 5.4.7  Copyright (C) 1994-2024 Lua.org, PUC-Rio` as its \
             version, not Lua 5.5.1: install PUC-Rio Lua 5.5.1, or set `FERROLUA_PUC_LUA` to its \
             path"
        );
    }

    #[test]
    fn says_when_the_wrong_lua_reports_no_version() {
        let error = Error::WrongLua {
            lua: PathBuf::from("lua"),
            reported: String::new(),
        };
        assert_eq!(
            error.to_string(),
            "`lua` reports no version, not Lua 5.5.1: install PUC-Rio Lua 5.5.1, or set \
             `FERROLUA_PUC_LUA` to its path"
        );
    }

    #[test]
    fn says_which_script_lua_failed_on_and_its_status() {
        let status = Command::new("rustc")
            .arg("--no-such-option")
            .output()
            .unwrap()
            .status;
        let error = Error::LuaFailed {
            lua: PathBuf::from("lua"),
            script: PathBuf::from("reference/goto.lua"),
            status,
        };
        assert_eq!(
            error.to_string(),
            format!("`lua` failed on `reference/goto.lua`: {status}")
        );
    }

    #[test]
    fn says_which_path_could_not_be_read_and_why() {
        let error = Error::Read {
            path: PathBuf::from("tests/conformance/reference"),
            source: io::Error::new(io::ErrorKind::NotFound, "not found"),
        };
        assert_eq!(
            error.to_string(),
            "couldn't read `tests/conformance/reference`: not found"
        );
    }

    #[test]
    fn says_which_out_file_could_not_be_saved_and_why() {
        let error = Error::Save {
            path: PathBuf::from("reference/goto.out"),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        };
        assert_eq!(
            error.to_string(),
            "couldn't save `reference/goto.out`: denied"
        );
    }

    #[test]
    fn says_where_the_reference_scripts_are() {
        let error = Error::NotReference {
            path: PathBuf::from("tests/conformance/goto.lua"),
        };
        assert_eq!(
            error.to_string(),
            "`tests/conformance/goto.lua` isn't a reference script: name a `.lua` file in \
             `tests/conformance/reference/`"
        );
    }

    #[test]
    fn says_which_folder_holds_no_scripts() {
        let error = Error::NoScripts {
            folder: PathBuf::from("tests/conformance/reference"),
        };
        assert_eq!(
            error.to_string(),
            "`tests/conformance/reference` holds no `.lua` scripts"
        );
    }
}
