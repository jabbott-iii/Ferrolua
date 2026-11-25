//! What the coverage and record tasks ask Cargo: where the workspace and its crates are, and which
//! test programs a build with coverage instrumentation makes.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::Error;
use crate::json::{self, Value};
use crate::process;

/// The Cargo workspace that holds the current folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Workspace {
    /// The folder of the workspace's root `Cargo.toml`.
    pub(crate) root: PathBuf,
    /// The folder Cargo builds into, `target/` unless `CARGO_TARGET_DIR` or a Cargo setting moves
    /// it.
    pub(crate) target: PathBuf,
    /// The workspace's member crates, sorted by name.
    pub(crate) crates: Vec<Crate>,
}

/// A member crate of a [`Workspace`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Crate {
    /// The package name, such as `lunaferra-testkit`.
    pub(crate) name: String,
    /// The folder of its `Cargo.toml`.
    pub(crate) dir: PathBuf,
}

impl Workspace {
    /// Asks `cargo metadata`, run as `cargo`, for the workspace that holds the current folder.
    ///
    /// # Errors
    ///
    /// Returns an error when `cargo metadata` can't start or fails, or when its output isn't the
    /// JSON this function reads.
    pub(crate) fn locate(cargo: &OsStr) -> Result<Workspace, Error> {
        let metadata = process::capture(
            "cargo metadata",
            Command::new(cargo)
                .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
                .stderr(Stdio::inherit()),
        )?;
        Workspace::from_metadata(&metadata)
    }

    fn from_metadata(metadata: &[u8]) -> Result<Workspace, Error> {
        let malformed = |problem: String| Error::Output {
            program: "cargo metadata",
            problem,
        };
        let metadata = json::parse(metadata).map_err(|error| malformed(error.to_string()))?;
        let (Some(root), Some(target), Some(packages)) = (
            metadata.get("workspace_root").and_then(Value::as_str),
            metadata.get("target_directory").and_then(Value::as_str),
            metadata.get("packages").and_then(Value::as_array),
        ) else {
            return Err(malformed(
                "no `workspace_root`, `target_directory`, or `packages`".to_owned(),
            ));
        };
        let mut crates = packages
            .iter()
            .map(Crate::from_package)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                malformed("a package without a `name`, or a `manifest_path` in a folder".to_owned())
            })?;
        crates.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Workspace {
            root: PathBuf::from(root),
            target: PathBuf::from(target),
            crates,
        })
    }
}

impl Crate {
    fn from_package(package: &Value) -> Option<Crate> {
        let (Some(name), Some(manifest)) = (
            package.get("name").and_then(Value::as_str),
            package.get("manifest_path").and_then(Value::as_str),
        ) else {
            return None;
        };
        Path::new(manifest).parent().map(|dir| Crate {
            name: name.to_owned(),
            dir: dir.to_owned(),
        })
    }
}

/// The test programs that a build with coverage instrumentation made, and where their runs wrote
/// their coverage profiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestRun {
    /// Every program the build made: the test programs, and the binaries the tests run.
    pub(crate) objects: Vec<PathBuf>,
    /// The folder of the profiles, one for each process that ran an instrumented program.
    pub(crate) profiles: PathBuf,
    /// Where the merged profile goes.
    pub(crate) profile: PathBuf,
}

impl TestRun {
    /// Builds and runs the workspace's tests with `-C instrument-coverage`, in a target folder of
    /// their own, `coverage/` in the workspace's target folder, so the instrumented build doesn't
    /// replace the usual one. The tests' output goes to standard error, so standard output carries
    /// only the report.
    ///
    /// The run leaves out doc tests, which stable Rust can't instrument, and removes the profiles
    /// of any earlier run first.
    ///
    /// # Errors
    ///
    /// Returns an error when the earlier profiles can't be removed, when the tests don't build or
    /// fail, or when the build makes no test program.
    pub(crate) fn run(cargo: &OsStr, workspace: &Workspace) -> Result<TestRun, Error> {
        let target = workspace.target.join("coverage");
        let profiles = target.join("profiles");
        remove_folder(&profiles)?;
        let messages = process::capture(
            "cargo test --no-run",
            cargo_test(cargo, &target, &profiles)
                .args(["--no-run", "--message-format=json-render-diagnostics"]),
        )?;
        process::capture(
            "cargo test",
            cargo_test(cargo, &target, &profiles).stdout(io::stderr()),
        )?;
        TestRun::from_messages(&messages, profiles, target.join("coverage.profdata"))
    }

    /// Reads the programs that Cargo built from its JSON messages, one on each line.
    fn from_messages(
        messages: &[u8],
        profiles: PathBuf,
        profile: PathBuf,
    ) -> Result<TestRun, Error> {
        let mut objects = Vec::new();
        for line in messages
            .split(|&byte| byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let message = json::parse(line).map_err(|error| Error::Output {
                program: "cargo test --no-run",
                problem: error.to_string(),
            })?;
            if message.get("reason").and_then(Value::as_str) == Some("compiler-artifact")
                && let Some(executable) = message.get("executable").and_then(Value::as_str)
            {
                objects.push(PathBuf::from(executable));
            }
        }
        if objects.is_empty() {
            Err(Error::NoTests)
        } else {
            Ok(TestRun {
                objects,
                profiles,
                profile,
            })
        }
    }
}

/// A `cargo test` command for every test program in the workspace, built with coverage
/// instrumentation into `target`, whose processes each write a profile into `profiles`.
fn cargo_test(cargo: &OsStr, target: &Path, profiles: &Path) -> Command {
    let mut command = Command::new(cargo);
    command
        .args(["test", "--workspace", "--locked", "--tests"])
        // Cargo reads `CARGO_ENCODED_RUSTFLAGS` before `RUSTFLAGS`, so a value set outside would win.
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("RUSTFLAGS", "-C instrument-coverage")
        .env("CARGO_TARGET_DIR", target)
        // `%p` is the process ID and `%m` an ID of the program. A later process that gets an earlier
        // one's ID merges its counts into that file, because of `%m`, instead of replacing it.
        .env("LLVM_PROFILE_FILE", profiles.join("%p-%m.profraw"))
        .stderr(Stdio::inherit());
    command
}

/// Removes `folder` and everything in it, if it exists.
fn remove_folder(folder: &Path) -> Result<(), Error> {
    match fs::remove_dir_all(folder) {
        Err(source) if source.kind() != io::ErrorKind::NotFound => Err(Error::Remove {
            path: folder.to_owned(),
            source,
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Crate, TestRun, Workspace};
    use crate::error::Error;

    #[test]
    fn reads_the_workspace_and_sorts_its_crates_by_name() {
        let metadata = br#"{
            "packages": [
                {"name": "xtask", "manifest_path": "/w/xtask/Cargo.toml", "version": "0.0.0"},
                {"name": "alpha", "manifest_path": "/w/crates/alpha/Cargo.toml"}
            ],
            "workspace_root": "/w",
            "target_directory": "/w/target"
        }"#;
        assert_eq!(
            Workspace::from_metadata(metadata).unwrap(),
            Workspace {
                root: PathBuf::from("/w"),
                target: PathBuf::from("/w/target"),
                crates: vec![
                    Crate {
                        name: "alpha".to_owned(),
                        dir: PathBuf::from("/w/crates/alpha"),
                    },
                    Crate {
                        name: "xtask".to_owned(),
                        dir: PathBuf::from("/w/xtask"),
                    },
                ],
            }
        );
    }

    #[test]
    fn rejects_metadata_that_is_not_json() {
        assert_eq!(
            Workspace::from_metadata(b"error").unwrap_err().to_string(),
            "unexpected output from `cargo metadata`: expected a value at byte 0"
        );
    }

    #[test]
    fn rejects_metadata_without_the_workspace_fields() {
        let metadata = br#"{"packages": [], "workspace_root": "/w"}"#;
        assert_eq!(
            Workspace::from_metadata(metadata).unwrap_err().to_string(),
            "unexpected output from `cargo metadata`: \
             no `workspace_root`, `target_directory`, or `packages`"
        );
    }

    #[test]
    fn rejects_a_package_without_a_name() {
        let metadata = br#"{
            "packages": [{"manifest_path": "/w/Cargo.toml"}],
            "workspace_root": "/w",
            "target_directory": "/w/target"
        }"#;
        assert_eq!(
            Workspace::from_metadata(metadata).unwrap_err().to_string(),
            "unexpected output from `cargo metadata`: \
             a package without a `name`, or a `manifest_path` in a folder"
        );
    }

    #[test]
    fn rejects_a_manifest_path_with_no_folder() {
        let metadata = br#"{
            "packages": [{"name": "alpha", "manifest_path": "/"}],
            "workspace_root": "/w",
            "target_directory": "/w/target"
        }"#;
        assert_eq!(
            Workspace::from_metadata(metadata).unwrap_err().to_string(),
            "unexpected output from `cargo metadata`: \
             a package without a `name`, or a `manifest_path` in a folder"
        );
    }

    #[test]
    fn collects_each_program_that_cargo_built() {
        let messages =
            br#"{"reason":"compiler-artifact","target":{"kind":["lib"]},"executable":null}
{"reason":"compiler-artifact","target":{"kind":["lib"]},"executable":"/w/target/alpha-1"}
{"reason":"compiler-artifact","target":{"kind":["bin"]},"executable":"/w/target/xtask"}
{"reason":"build-script-executed","executable":"/w/target/not-an-artifact"}

{"reason":"build-finished","success":true}
"#;
        let run = TestRun::from_messages(
            messages,
            PathBuf::from("/w/profiles"),
            PathBuf::from("/w/coverage.profdata"),
        )
        .unwrap();
        assert_eq!(
            run,
            TestRun {
                objects: vec![
                    PathBuf::from("/w/target/alpha-1"),
                    PathBuf::from("/w/target/xtask"),
                ],
                profiles: PathBuf::from("/w/profiles"),
                profile: PathBuf::from("/w/coverage.profdata"),
            }
        );
    }

    #[test]
    fn rejects_a_message_that_is_not_json() {
        let messages = b"{\"reason\":\"build-finished\"}\nwarning: not JSON\n";
        let result = TestRun::from_messages(messages, PathBuf::new(), PathBuf::new());
        assert_eq!(
            result.unwrap_err().to_string(),
            "unexpected output from `cargo test --no-run`: expected a value at byte 0"
        );
    }

    #[test]
    fn fails_when_cargo_built_no_program() {
        let messages = b"{\"reason\":\"build-finished\",\"success\":true}\n";
        let result = TestRun::from_messages(messages, PathBuf::new(), PathBuf::new());
        assert!(matches!(result, Err(Error::NoTests)));
    }
}
