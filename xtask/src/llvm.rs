//! The LLVM tools that measure coverage, `llvm-profdata` and `llvm-cov` from the toolchain's
//! `llvm-tools` component, and the coverage they report.
//!
//! `llvm-cov report` counts a function that was compiled more than once, such as a generic
//! function called with two closures, or a library function in both its unit-test program and an
//! integration-test program, by its one copy that ran the most regions. A region that only another
//! copy ran then shows as missed. [`Coverage`] instead counts a region as covered when any copy
//! ran it, and a line as covered when `llvm-cov`'s view of the whole file, which adds up the counts
//! of every copy, shows that it ran.

use std::collections::BTreeMap;
use std::env::consts::EXE_SUFFIX;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str;

use crate::cargo::TestRun;
use crate::error::Error;
use crate::json::{self, Value};
use crate::process;

/// The kind number that `llvm-cov export` gives a code region, the kind it counts. The others are
/// expansion, skipped, gap, and branch regions.
const CODE_REGION: u64 = 0;

/// Where the toolchain's `llvm-profdata` and `llvm-cov` are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Llvm {
    profdata: PathBuf,
    cov: PathBuf,
}

impl Llvm {
    /// Finds the tools in the sysroot of `rustc`, in `lib/rustlib/<host>/bin/`, where rustup
    /// installs the `llvm-tools` component.
    ///
    /// # Errors
    ///
    /// Returns an error when `rustc` can't start or fails, when its output isn't two lines, or when
    /// a tool isn't installed.
    pub(crate) fn locate(rustc: &OsStr) -> Result<Llvm, Error> {
        let output = process::capture(
            "rustc",
            Command::new(rustc)
                .args(["--print", "sysroot", "--print", "host-tuple"])
                .stderr(Stdio::inherit()),
        )?;
        Llvm::in_sysroot(&output)
    }

    /// Finds the tools from what `rustc --print sysroot --print host-tuple` printed.
    fn in_sysroot(rustc_output: &[u8]) -> Result<Llvm, Error> {
        let text = String::from_utf8_lossy(rustc_output);
        let mut lines = text.lines();
        let (Some(sysroot), Some(host), None) = (lines.next(), lines.next(), lines.next()) else {
            return Err(Error::Output {
                program: "rustc",
                problem: "expected two lines, the sysroot and the host".to_owned(),
            });
        };
        let bin = Path::new(sysroot)
            .join("lib")
            .join("rustlib")
            .join(host)
            .join("bin");
        let llvm = Llvm {
            profdata: bin.join(format!("llvm-profdata{EXE_SUFFIX}")),
            cov: bin.join(format!("llvm-cov{EXE_SUFFIX}")),
        };
        let missing = [&llvm.profdata, &llvm.cov]
            .into_iter()
            .find(|tool| !tool.is_file())
            .cloned();
        match missing {
            Some(path) => Err(Error::MissingTool { path }),
            None => Ok(llvm),
        }
    }

    /// Merges the profiles of `run` and reads which lines and regions its tests ran.
    ///
    /// # Errors
    ///
    /// Returns an error when a tool can't start or fails, for example because the run wrote no
    /// profiles, or when the output of `llvm-cov export` isn't in the form [`Coverage`] reads.
    pub(crate) fn measure(&self, run: &TestRun) -> Result<Coverage, Error> {
        process::capture(
            "llvm-profdata merge",
            Command::new(&self.profdata)
                .args(["merge", "-sparse"])
                .arg(&run.profiles)
                .arg("-o")
                .arg(&run.profile)
                .stderr(Stdio::inherit()),
        )?;
        let json = process::capture("llvm-cov export", &mut self.export(run, "text"))?;
        // This second export fails only where the first would, so `and_then` carries its error.
        process::capture("llvm-cov export", &mut self.export(run, "lcov"))
            .and_then(|lcov| Coverage::from_exports(&json, &lcov))
    }

    /// An `llvm-cov export` command that writes the coverage of `run` in `format`: `text` for
    /// JSON, or `lcov`.
    fn export(&self, run: &TestRun, format: &str) -> Command {
        let mut command = Command::new(&self.cov);
        command
            .arg("export")
            .arg(format!("-format={format}"))
            .arg("-instr-profile")
            .arg(&run.profile)
            .stderr(Stdio::inherit());
        for object in &run.objects {
            command.arg("-object").arg(object);
        }
        command
    }
}

/// Which lines and regions of each source file the tests ran, merged over every test program and
/// every copy of each function.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Coverage {
    /// Each file that has code, by its absolute path.
    pub(crate) files: BTreeMap<PathBuf, FileCoverage>,
}

/// Which lines and regions of one source file the tests ran.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct FileCoverage {
    /// Each line that `llvm-cov` counts, by its number from 1, and whether a test ran it.
    pub(crate) lines: BTreeMap<u64, bool>,
    /// Each code region, and whether a test ran it.
    pub(crate) regions: BTreeMap<Region, bool>,
}

/// A span of source code that the compiler counts as one unit, such as a function's body up to
/// its first branch, or one arm of a `match`. Every `?` is a region of its own, which runs only
/// when it returns an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Region {
    /// The line and column where it starts, both from 1.
    pub(crate) start: (u64, u64),
    /// The line and column where it ends.
    pub(crate) end: (u64, u64),
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ((start_line, start_column), (end_line, end_column)) = (self.start, self.end);
        write!(f, "{start_line}:{start_column}-{end_line}:{end_column}")
    }
}

impl Coverage {
    /// Reads the regions from the JSON that `llvm-cov export -format=text` printed, and the lines
    /// from what `llvm-cov export -format=lcov` printed, for the same profile and programs.
    fn from_exports(json: &[u8], lcov: &[u8]) -> Result<Coverage, Error> {
        let mut coverage = Coverage::default();
        coverage.add_regions(json)?;
        coverage.add_lines(lcov).map(|()| coverage)
    }

    /// Adds each code region of each function in the JSON export, covered when any copy of the
    /// function ran it.
    fn add_regions(&mut self, json: &[u8]) -> Result<(), Error> {
        let export = json::parse(json).map_err(|error| malformed(&error.to_string()))?;
        let Some(data) = export.get("data").and_then(Value::as_array) else {
            return Err(malformed("no `data` array"));
        };
        for unit in data {
            let Some(functions) = unit.get("functions").and_then(Value::as_array) else {
                return Err(malformed("no `functions` array"));
            };
            for function in functions {
                let (Some(filenames), Some(regions)) = (
                    function.get("filenames").and_then(Value::as_array),
                    function.get("regions").and_then(Value::as_array),
                ) else {
                    return Err(malformed("a function without `filenames` or `regions`"));
                };
                for region in regions {
                    let Some((file, region, count, kind)) = parse_region(region, filenames) else {
                        return Err(malformed(
                            "a region that isn't 8 integers, or names a file its function lacks",
                        ));
                    };
                    if kind == CODE_REGION {
                        let file = self.files.entry(PathBuf::from(file)).or_default();
                        *file.regions.entry(region).or_default() |= count > 0;
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds each line of each file in the lcov export: an `SF:<path>` record starts a file, and a
    /// `DA:<line>,<count>` record gives how often a line ran.
    fn add_lines(&mut self, lcov: &[u8]) -> Result<(), Error> {
        let text = str::from_utf8(lcov).map_err(|_| malformed("lcov output that isn't UTF-8"))?;
        let mut file = None;
        for line in text.lines() {
            if let Some(path) = line.strip_prefix("SF:") {
                file = Some(self.files.entry(PathBuf::from(path)).or_default());
            } else if let Some(record) = line.strip_prefix("DA:") {
                let (Some(file), Some((number, count))) =
                    (file.as_deref_mut(), parse_line_record(record))
                else {
                    return Err(malformed(
                        "a `DA` record that isn't two integers, or comes before any `SF` record",
                    ));
                };
                *file.lines.entry(number).or_default() |= count > 0;
            }
        }
        Ok(())
    }
}

fn malformed(problem: &str) -> Error {
    Error::Output {
        program: "llvm-cov export",
        problem: problem.to_owned(),
    }
}

/// Reads a region of a function in the JSON export: an array of its start line and column, its end
/// line and column, how often it ran, the index of its file among the function's `filenames`, the
/// index of the file a macro expansion region expands, and its kind. Returns the file, the region,
/// the count, and the kind.
fn parse_region<'a>(region: &Value, filenames: &'a [Value]) -> Option<(&'a str, Region, u64, u64)> {
    let fields = region
        .as_array()?
        .iter()
        .map(Value::as_u64)
        .collect::<Option<Vec<_>>>()?;
    let [
        start_line,
        start_column,
        end_line,
        end_column,
        count,
        file,
        _,
        kind,
    ] = fields[..]
    else {
        return None;
    };
    let file = usize::try_from(file)
        .ok()
        .and_then(|index| filenames.get(index))?
        .as_str()?;
    let region = Region {
        start: (start_line, start_column),
        end: (end_line, end_column),
    };
    Some((file, region, count, kind))
}

/// Reads the `<line>,<count>` of a `DA` record.
fn parse_line_record(record: &str) -> Option<(u64, u64)> {
    let (line, count) = record.split_once(',')?;
    Some((line.parse().ok()?, count.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::env;
    use std::env::consts::EXE_SUFFIX;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{Coverage, FileCoverage, Llvm, Region, parse_line_record, parse_region};
    use crate::cargo::TestRun;
    use crate::error::Error;
    use crate::json;

    /// A new, empty folder for one test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lunaferra-xtask-{}-{name}", std::process::id()));
        // The folder is left from an earlier run only if that run stopped early.
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn region(start: (u64, u64), end: (u64, u64)) -> Region {
        Region { start, end }
    }

    /// A sysroot in a folder named `name` that holds `tools`, and the `rustc` output that names it.
    fn sysroot_with(name: &str, tools: &[&str]) -> (PathBuf, String) {
        let sysroot = temp_dir(name);
        let bin = sysroot.join("lib/rustlib/x86_64-unknown-linux-gnu/bin");
        fs::create_dir_all(&bin).unwrap();
        for tool in tools {
            fs::write(bin.join(format!("{tool}{EXE_SUFFIX}")), b"").unwrap();
        }
        let output = format!("{}\nx86_64-unknown-linux-gnu\n", sysroot.display());
        (sysroot, output)
    }

    #[test]
    fn finds_the_tools_in_the_sysroot_for_the_host() {
        let (sysroot, output) = sysroot_with("sysroot-complete", &["llvm-profdata", "llvm-cov"]);
        let bin = sysroot.join("lib/rustlib/x86_64-unknown-linux-gnu/bin");
        assert_eq!(
            Llvm::in_sysroot(output.as_bytes()).unwrap(),
            Llvm {
                profdata: bin.join(format!("llvm-profdata{EXE_SUFFIX}")),
                cov: bin.join(format!("llvm-cov{EXE_SUFFIX}")),
            }
        );
        fs::remove_dir_all(sysroot).unwrap();
    }

    #[test]
    fn says_which_tool_is_missing() {
        let (sysroot, output) = sysroot_with("sysroot-empty", &[]);
        let error = Llvm::in_sysroot(output.as_bytes()).unwrap_err();
        let profdata = sysroot.join(format!(
            "lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata{EXE_SUFFIX}"
        ));
        assert!(matches!(error, Error::MissingTool { path } if path == profdata));
        fs::remove_dir_all(sysroot).unwrap();

        let (sysroot, output) = sysroot_with("sysroot-profdata", &["llvm-profdata"]);
        let error = Llvm::in_sysroot(output.as_bytes()).unwrap_err();
        let cov = sysroot.join(format!(
            "lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-cov{EXE_SUFFIX}"
        ));
        assert!(matches!(error, Error::MissingTool { path } if path == cov));
        fs::remove_dir_all(sysroot).unwrap();
    }

    #[test]
    fn rejects_rustc_output_that_is_not_two_lines() {
        for output in ["", "/sysroot\n", "/sysroot\nhost\nextra\n"] {
            assert_eq!(
                Llvm::in_sysroot(output.as_bytes()).unwrap_err().to_string(),
                "unexpected output from `rustc`: expected two lines, the sysroot and the host"
            );
        }
    }

    // This test and the next run the real tools, which print their own `error:` line to the test's
    // standard error before `measure` returns the failure.
    #[test]
    fn fails_when_there_are_no_profiles_to_merge() {
        let dir = temp_dir("no-profiles");
        let llvm = Llvm::locate(OsStr::new("rustc")).unwrap();
        let run = TestRun {
            objects: Vec::new(),
            profiles: dir.join("profiles"),
            profile: dir.join("coverage.profdata"),
        };
        let error = llvm.measure(&run).unwrap_err();
        assert!(matches!(
            error,
            Error::Failed { program: "llvm-profdata merge", status } if status.code() == Some(1)
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn fails_when_a_program_to_export_does_not_exist() {
        let dir = temp_dir("no-program");
        let profiles = dir.join("profiles");
        fs::create_dir(&profiles).unwrap();
        // An empty profile in `llvm-profdata`'s text format, which merges without error.
        fs::write(profiles.join("empty.proftext"), ":ir\n").unwrap();
        let llvm = Llvm::locate(OsStr::new("rustc")).unwrap();
        let run = TestRun {
            objects: vec![dir.join("missing")],
            profiles,
            profile: dir.join("coverage.profdata"),
        };
        let error = llvm.measure(&run).unwrap_err();
        assert!(matches!(
            error,
            Error::Failed { program: "llvm-cov export", status } if status.code() == Some(1)
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn counts_a_region_as_covered_when_any_copy_of_its_function_ran_it() {
        // Two copies of one generic function, each running one branch, and a gap region, which
        // isn't counted.
        let json = br#"{"data": [{"functions": [
            {"filenames": ["/w/a/src/lib.rs"], "regions": [
                [1, 1, 3, 2, 1, 0, 0, 0], [2, 8, 2, 9, 1, 0, 0, 0], [2, 12, 2, 13, 0, 0, 0, 0],
                [2, 10, 2, 11, 0, 0, 0, 3]
            ]},
            {"filenames": ["/w/a/src/lib.rs"], "regions": [
                [1, 1, 3, 2, 1, 0, 0, 0], [2, 8, 2, 9, 0, 0, 0, 0], [2, 12, 2, 13, 5, 0, 0, 0]
            ]},
            {"filenames": ["/w/a/src/main.rs", "/w/a/src/other.rs"], "regions": [
                [7, 1, 7, 9, 0, 1, 0, 0]
            ]}
        ]}]}"#;
        let lcov = b"SF:/w/a/src/lib.rs\nFN:1,f\nDA:1,2\nDA:2,2\nDA:3,2\nLF:3\nLH:3\n\
            end_of_record\nSF:/w/a/src/other.rs\nDA:7,0\nend_of_record\n";
        let coverage = Coverage::from_exports(json, lcov).unwrap();
        assert_eq!(
            coverage,
            Coverage {
                files: BTreeMap::from([
                    (
                        PathBuf::from("/w/a/src/lib.rs"),
                        FileCoverage {
                            lines: BTreeMap::from([(1, true), (2, true), (3, true)]),
                            regions: BTreeMap::from([
                                (region((1, 1), (3, 2)), true),
                                (region((2, 8), (2, 9)), true),
                                (region((2, 12), (2, 13)), true),
                            ]),
                        }
                    ),
                    (
                        PathBuf::from("/w/a/src/other.rs"),
                        FileCoverage {
                            lines: BTreeMap::from([(7, false)]),
                            regions: BTreeMap::from([(region((7, 1), (7, 9)), false)]),
                        }
                    ),
                ]),
            }
        );
    }

    #[test]
    fn counts_a_line_as_covered_when_any_record_of_it_ran() {
        let mut coverage = Coverage::default();
        coverage
            .add_lines(b"SF:/w/a.rs\nDA:4,0\nDA:5,0\nSF:/w/a.rs\nDA:4,3\n")
            .unwrap();
        assert_eq!(
            coverage.files[Path::new("/w/a.rs")].lines,
            BTreeMap::from([(4, true), (5, false)])
        );
    }

    #[test]
    fn rejects_a_json_export_that_is_not_json() {
        assert_eq!(
            Coverage::from_exports(b"{", b"").unwrap_err().to_string(),
            "unexpected output from `llvm-cov export`: expected a member name at byte 1"
        );
    }

    #[test]
    fn rejects_a_json_export_without_its_arrays() {
        let cases: [(&[u8], &str); 3] = [
            (b"{}", "no `data` array"),
            (br#"{"data": [{}]}"#, "no `functions` array"),
            (
                br#"{"data": [{"functions": [{"regions": []}]}]}"#,
                "a function without `filenames` or `regions`",
            ),
        ];
        for (json, problem) in cases {
            assert_eq!(
                Coverage::from_exports(json, b"").unwrap_err().to_string(),
                format!("unexpected output from `llvm-cov export`: {problem}")
            );
        }
    }

    #[test]
    fn rejects_a_malformed_region() {
        let json = br#"{"data": [{"functions": [{"filenames": ["/w/a.rs"], "regions": [[1]]}]}]}"#;
        assert_eq!(
            Coverage::from_exports(json, b"").unwrap_err().to_string(),
            "unexpected output from `llvm-cov export`: \
             a region that isn't 8 integers, or names a file its function lacks"
        );
    }

    #[test]
    fn reads_a_region_only_from_8_integers_that_name_a_string_file() {
        let filenames = [json::Value::String("/w/a.rs".to_owned()), json::Value::Null];
        let parse = |text: &str| parse_region(&json::parse(text.as_bytes()).unwrap(), &filenames);
        assert_eq!(
            parse("[1, 2, 3, 4, 5, 0, 0, 0]"),
            Some(("/w/a.rs", region((1, 2), (3, 4)), 5, 0))
        );
        assert_eq!(parse("{}"), None);
        assert_eq!(parse("[1, 2, 3, 4, 5, 0, 0, -1]"), None);
        assert_eq!(parse("[1, 2, 3, 4, 5, 0, 0]"), None);
        assert_eq!(parse("[1, 2, 3, 4, 5, 0, 0, 0, 0]"), None);
        assert_eq!(parse("[1, 2, 3, 4, 5, 2, 0, 0]"), None);
        assert_eq!(parse("[1, 2, 3, 4, 5, 1, 0, 0]"), None);
    }

    #[test]
    fn rejects_an_lcov_export_that_is_not_utf8() {
        assert_eq!(
            Coverage::from_exports(br#"{"data": []}"#, b"SF:\xff\n")
                .unwrap_err()
                .to_string(),
            "unexpected output from `llvm-cov export`: lcov output that isn't UTF-8"
        );
    }

    #[test]
    fn rejects_a_line_record_before_a_file_or_with_malformed_numbers() {
        for lcov in ["DA:1,1\n", "SF:/w/a.rs\nDA:1\n"] {
            assert_eq!(
                Coverage::from_exports(br#"{"data": []}"#, lcov.as_bytes())
                    .unwrap_err()
                    .to_string(),
                "unexpected output from `llvm-cov export`: \
                 a `DA` record that isn't two integers, or comes before any `SF` record"
            );
        }
    }

    #[test]
    fn reads_a_line_record_only_from_two_integers() {
        assert_eq!(parse_line_record("12,3"), Some((12, 3)));
        assert_eq!(parse_line_record("12"), None);
        assert_eq!(parse_line_record("x,3"), None);
        assert_eq!(parse_line_record("12,x"), None);
    }

    #[test]
    fn shows_a_region_as_its_start_and_end() {
        assert_eq!(region((9, 1), (11, 2)).to_string(), "9:1-11:2");
    }
}
