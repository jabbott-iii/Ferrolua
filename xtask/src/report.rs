//! The coverage report: how many lines and regions of each crate the tests ran, and where the
//! ones they didn't run are.

use std::fmt;
use std::iter;
use std::path::{Component, Path, PathBuf};

use crate::cargo::{Crate, Workspace};
use crate::llvm::{Coverage, FileCoverage, Region};

/// The folders of a crate that the report doesn't count, because they hold the code that measures
/// the crate rather than code to measure: its integration tests, benchmarks, and examples. Some
/// of their code never runs by design, such as the end of a test that expects a panic.
const TEST_FOLDERS: [&str; 3] = ["tests", "benches", "examples"];

/// How much of each crate of a workspace the tests ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Report {
    /// One row for each crate, in the order of the workspace's crates.
    rows: Vec<Row>,
    /// Each counted file with a line or region that no test ran, in the order of their paths.
    misses: Vec<Miss>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    name: String,
    lines: Tally,
    regions: Tally,
}

/// How many lines or regions there are, and how many of them ran.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Tally {
    covered: u64,
    total: u64,
}

/// The lines and regions of one file that no test ran.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Miss {
    /// The file, relative to the workspace's root.
    path: PathBuf,
    lines: Vec<u64>,
    regions: Vec<Region>,
}

impl Report {
    /// Counts the lines and regions of each crate in `workspace`, from `coverage`. Each file
    /// belongs to the crate whose folder holds it, and to the innermost one when crate folders
    /// nest. Files outside every crate, such as the standard library's, and files in a crate's
    /// [`TEST_FOLDERS`] aren't counted.
    pub(crate) fn new(workspace: &Workspace, coverage: &Coverage) -> Report {
        let counted: Vec<(usize, &Path, &FileCoverage)> = coverage
            .files
            .iter()
            .filter_map(|(path, file)| {
                owner(&workspace.crates, path).map(|index| (index, path.as_path(), file))
            })
            .collect();
        let rows = workspace
            .crates
            .iter()
            .enumerate()
            .map(|(index, krate)| {
                let files: Vec<&FileCoverage> = counted
                    .iter()
                    .filter(|(owner, ..)| *owner == index)
                    .map(|&(_, _, file)| file)
                    .collect();
                Row {
                    name: krate.name.clone(),
                    lines: Tally::of(files.iter().flat_map(|file| file.lines.values())),
                    regions: Tally::of(files.iter().flat_map(|file| file.regions.values())),
                }
            })
            .collect();
        let misses = counted
            .iter()
            .filter_map(|&(_, path, file)| {
                Miss::of(path.strip_prefix(&workspace.root).unwrap_or(path), file)
            })
            .collect();
        Report { rows, misses }
    }

    /// Says whether the tests ran every counted line and region.
    pub(crate) fn passed(&self) -> bool {
        self.misses.is_empty()
    }

    /// Writes the report as text: a table of each crate's lines and regions with a total, the
    /// lines and regions that no test ran, and a last line that says whether coverage is 100%.
    pub(crate) fn render(&self) -> String {
        let total = Row {
            name: "total".to_owned(),
            lines: self
                .rows
                .iter()
                .map(|row| row.lines)
                .fold(Tally::default(), Tally::add),
            regions: self
                .rows
                .iter()
                .map(|row| row.regions)
                .fold(Tally::default(), Tally::add),
        };
        let header = ["crate".to_owned(), "lines".to_owned(), "regions".to_owned()];
        let cells: Vec<[String; 3]> = iter::once(header)
            .chain(self.rows.iter().chain([&total]).map(|row| {
                [
                    row.name.clone(),
                    row.lines.to_string(),
                    row.regions.to_string(),
                ]
            }))
            .collect();
        let name_width = cells
            .iter()
            .map(|[name, _, _]| name.len())
            .max()
            .unwrap_or_default();
        let lines_width = cells
            .iter()
            .map(|[_, lines, _]| lines.len())
            .max()
            .unwrap_or_default();
        let regions_width = cells
            .iter()
            .map(|[_, _, regions]| regions.len())
            .max()
            .unwrap_or_default();
        let mut text: String = cells
            .iter()
            .map(|[name, lines, regions]| {
                format!("{name:<name_width$}  {lines:>lines_width$}  {regions:>regions_width$}\n")
            })
            .collect();
        if self.passed() {
            text.push_str("\nevery line and region is covered\n");
        } else {
            text.push_str("\nnot covered:\n");
            for miss in &self.misses {
                text.push_str(&miss.render());
            }
            text.push_str("\ncoverage is below 100% of lines and regions\n");
        }
        text
    }
}

/// Finds the index of the crate that `path` belongs to: the innermost crate whose folder holds it,
/// unless `path` is in one of that crate's [`TEST_FOLDERS`].
fn owner(crates: &[Crate], path: &Path) -> Option<usize> {
    let (index, inside) = crates
        .iter()
        .enumerate()
        .filter_map(|(index, krate)| {
            path.strip_prefix(&krate.dir)
                .ok()
                .map(|inside| (index, inside))
        })
        .min_by_key(|(_, inside)| inside.components().count())?;
    let in_test_folder = matches!(
        inside.components().next(),
        Some(Component::Normal(folder)) if TEST_FOLDERS.iter().any(|test| folder == *test)
    );
    (!in_test_folder).then_some(index)
}

impl Tally {
    fn of<'a>(ran: impl Iterator<Item = &'a bool>) -> Tally {
        ran.fold(Tally::default(), |tally, &ran| Tally {
            covered: tally.covered + u64::from(ran),
            total: tally.total + 1,
        })
    }

    fn add(self, other: Tally) -> Tally {
        Tally {
            covered: self.covered + other.covered,
            total: self.total + other.total,
        }
    }
}

impl fmt::Display for Tally {
    /// Writes `<covered>/<total>` and the percentage covered, rounded down so that only a complete
    /// tally shows 100.00%, or `-` when there is nothing to count.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let percent = if self.total == 0 {
            "-".to_owned()
        } else {
            let hundredths = u128::from(self.covered) * 10_000 / u128::from(self.total);
            format!("{}.{:02}%", hundredths / 100, hundredths % 100)
        };
        write!(f, "{}/{} {percent:>7}", self.covered, self.total)
    }
}

impl Miss {
    /// Collects the lines and regions of `file` that no test ran, if there are any.
    fn of(path: &Path, file: &FileCoverage) -> Option<Miss> {
        let lines: Vec<u64> = file
            .lines
            .iter()
            .filter(|&(_, &ran)| !ran)
            .map(|(&line, _)| line)
            .collect();
        let regions: Vec<Region> = file
            .regions
            .iter()
            .filter(|&(_, &ran)| !ran)
            .map(|(&region, _)| region)
            .collect();
        (!lines.is_empty() || !regions.is_empty()).then(|| Miss {
            path: path.to_owned(),
            lines,
            regions,
        })
    }

    /// Writes one line: the file, then its lines as ranges of consecutive numbers, then its
    /// regions.
    fn render(&self) -> String {
        let mut parts = Vec::new();
        if !self.lines.is_empty() {
            parts.push(format!("lines {}", line_ranges(&self.lines)));
        }
        if !self.regions.is_empty() {
            let regions: Vec<String> = self.regions.iter().map(Region::to_string).collect();
            parts.push(format!("regions {}", regions.join(", ")));
        }
        format!("  {}: {}\n", self.path.display(), parts.join("; "))
    }
}

/// Writes sorted line numbers as ranges of consecutive ones, such as `9-11, 15`.
fn line_ranges(lines: &[u64]) -> String {
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for &line in lines {
        match ranges.last_mut() {
            Some((_, end)) if end.checked_add(1) == Some(line) => *end = line,
            _ => ranges.push((line, line)),
        }
    }
    let ranges: Vec<String> = ranges
        .iter()
        .map(|&(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect();
    ranges.join(", ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{Miss, Report, Row, Tally, line_ranges};
    use crate::cargo::{Crate, Workspace};
    use crate::llvm::{Coverage, FileCoverage, Region};

    fn region(start: (u64, u64), end: (u64, u64)) -> Region {
        Region { start, end }
    }

    fn file(lines: &[(u64, bool)], regions: &[(Region, bool)]) -> FileCoverage {
        FileCoverage {
            lines: lines.iter().copied().collect(),
            regions: regions.iter().copied().collect(),
        }
    }

    fn workspace() -> Workspace {
        let krate = |name: &str, dir: &str| Crate {
            name: name.to_owned(),
            dir: PathBuf::from(dir),
        };
        Workspace {
            root: PathBuf::from("/w"),
            target: PathBuf::from("/w/target"),
            crates: vec![
                krate("alpha", "/w/alpha"),
                krate("empty", "/w/empty"),
                krate("outer", "/w/outer"),
                krate("inner", "/w/outer/inner"),
            ],
        }
    }

    fn tally(covered: u64, total: u64) -> Tally {
        Tally { covered, total }
    }

    #[test]
    fn counts_each_file_in_the_innermost_crate_that_holds_it() {
        let coverage = Coverage {
            files: BTreeMap::from([
                (
                    PathBuf::from("/w/alpha/src/lib.rs"),
                    file(&[(1, true), (2, true)], &[(region((1, 1), (2, 2)), true)]),
                ),
                (
                    PathBuf::from("/w/outer/src/lib.rs"),
                    file(&[(1, true)], &[(region((1, 1), (1, 9)), true)]),
                ),
                (
                    PathBuf::from("/w/outer/inner/src/lib.rs"),
                    file(&[(1, true), (2, true), (3, true)], &[]),
                ),
                // Neither a test folder nor a file outside every crate counts.
                (
                    PathBuf::from("/w/alpha/tests/it.rs"),
                    file(&[(1, false)], &[(region((1, 1), (1, 9)), false)]),
                ),
                (
                    PathBuf::from("/w/outer/benches/b.rs"),
                    file(&[(1, false)], &[]),
                ),
                (
                    PathBuf::from("/w/outer/examples/e.rs"),
                    file(&[(1, false)], &[]),
                ),
                (
                    PathBuf::from("/rustc/1234/library/core/src/lib.rs"),
                    file(&[(1, false)], &[]),
                ),
            ]),
        };
        let report = Report::new(&workspace(), &coverage);
        let row = |name: &str, lines, regions| Row {
            name: name.to_owned(),
            lines,
            regions,
        };
        assert_eq!(
            report,
            Report {
                rows: vec![
                    row("alpha", tally(2, 2), tally(1, 1)),
                    row("empty", tally(0, 0), tally(0, 0)),
                    row("outer", tally(1, 1), tally(1, 1)),
                    row("inner", tally(3, 3), tally(0, 0)),
                ],
                misses: Vec::new(),
            }
        );
        assert!(report.passed());
    }

    #[test]
    fn lists_the_lines_and_regions_no_test_ran() {
        let coverage = Coverage {
            files: BTreeMap::from([
                (
                    PathBuf::from("/w/alpha/src/a.rs"),
                    file(
                        &[(1, true), (2, false)],
                        &[
                            (region((1, 1), (1, 5)), true),
                            (region((2, 1), (2, 5)), false),
                        ],
                    ),
                ),
                // A line that ran can hold a region that didn't, such as a `?` that never
                // returned an error.
                (
                    PathBuf::from("/w/alpha/src/b.rs"),
                    file(&[(1, true)], &[(region((1, 9), (1, 10)), false)]),
                ),
                (
                    PathBuf::from("/w/alpha/src/c.rs"),
                    file(&[(1, true)], &[(region((1, 1), (1, 5)), true)]),
                ),
            ]),
        };
        let report = Report::new(&workspace(), &coverage);
        assert_eq!(
            report.misses,
            vec![
                Miss {
                    path: PathBuf::from("alpha/src/a.rs"),
                    lines: vec![2],
                    regions: vec![region((2, 1), (2, 5))],
                },
                Miss {
                    path: PathBuf::from("alpha/src/b.rs"),
                    lines: Vec::new(),
                    regions: vec![region((1, 9), (1, 10))],
                },
            ]
        );
        assert!(!report.passed());
    }

    #[test]
    fn shows_a_file_outside_the_root_by_its_full_path() {
        let mut workspace = workspace();
        workspace.crates.push(Crate {
            name: "elsewhere".to_owned(),
            dir: PathBuf::from("/x/elsewhere"),
        });
        let coverage = Coverage {
            files: BTreeMap::from([(
                PathBuf::from("/x/elsewhere/src/lib.rs"),
                file(&[(1, false)], &[]),
            )]),
        };
        let report = Report::new(&workspace, &coverage);
        assert_eq!(
            report.misses[0].path,
            PathBuf::from("/x/elsewhere/src/lib.rs")
        );
    }

    #[test]
    fn renders_a_passing_report_with_aligned_columns_and_a_total() {
        let row = |name: &str, lines, regions| Row {
            name: name.to_owned(),
            lines,
            regions,
        };
        let report = Report {
            rows: vec![
                row("alpha", tally(12, 12), tally(20, 20)),
                row("lunaferra-none", tally(0, 0), tally(0, 0)),
                row("beta", tally(3, 3), tally(5, 5)),
            ],
            misses: Vec::new(),
        };
        assert_eq!(
            report.render(),
            "crate                   lines        regions\n\
             alpha           12/12 100.00%  20/20 100.00%\n\
             lunaferra-none    0/0       -    0/0       -\n\
             beta              3/3 100.00%    5/5 100.00%\n\
             total           15/15 100.00%  25/25 100.00%\n\
             \n\
             every line and region is covered\n"
        );
    }

    #[test]
    fn renders_a_failing_report_with_what_no_test_ran() {
        let report = Report {
            rows: vec![Row {
                name: "beta".to_owned(),
                lines: tally(6, 9),
                regions: tally(2, 3),
            }],
            misses: vec![
                Miss {
                    path: PathBuf::from("beta/src/lib.rs"),
                    lines: vec![9, 10, 11],
                    regions: vec![region((9, 1), (9, 32)), region((10, 5), (10, 10))],
                },
                Miss {
                    path: PathBuf::from("beta/src/b.rs"),
                    lines: Vec::new(),
                    regions: vec![region((1, 9), (1, 10))],
                },
                Miss {
                    path: PathBuf::from("beta/src/c.rs"),
                    lines: vec![4],
                    regions: Vec::new(),
                },
            ],
        };
        assert_eq!(
            report.render(),
            "crate        lines      regions\n\
             beta   6/9  66.66%  2/3  66.66%\n\
             total  6/9  66.66%  2/3  66.66%\n\
             \n\
             not covered:\n  \
             beta/src/lib.rs: lines 9-11; regions 9:1-9:32, 10:5-10:10\n  \
             beta/src/b.rs: regions 1:9-1:10\n  \
             beta/src/c.rs: lines 4\n\
             \n\
             coverage is below 100% of lines and regions\n"
        );
    }

    #[test]
    fn rounds_a_percentage_down() {
        assert_eq!(tally(1, 3).to_string(), "1/3  33.33%");
        assert_eq!(tally(9999, 10000).to_string(), "9999/10000  99.99%");
        assert_eq!(
            tally(u64::MAX, u64::MAX).to_string(),
            format!("{0}/{0} 100.00%", u64::MAX)
        );
    }

    #[test]
    fn writes_consecutive_lines_as_ranges() {
        assert_eq!(line_ranges(&[]), "");
        assert_eq!(line_ranges(&[7]), "7");
        assert_eq!(line_ranges(&[9, 10, 11, 15, 17, 18]), "9-11, 15, 17-18");
        assert_eq!(
            line_ranges(&[u64::MAX - 1, u64::MAX]),
            format!("{}-{}", u64::MAX - 1, u64::MAX)
        );
        assert_eq!(line_ranges(&[u64::MAX, 0]), format!("{}, 0", u64::MAX));
    }
}
