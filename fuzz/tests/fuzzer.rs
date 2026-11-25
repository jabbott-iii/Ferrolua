//! Runs the `ferrolua-fuzz` program the way `cargo xtask fuzz` does, and checks what it reports and
//! saves. Each test keeps its files in a folder of its own under the target folder, so no test
//! writes to `fuzz/corpus/` or `fuzz/regressions/`, except the one that replays every input saved
//! there, which only reads them.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The name that the fuzzer saves `FUZZ` under: its FNV-1a hash, computed apart from the fuzzer
/// with a few lines of Python.
const FUZZ: &str = "c6d0a3857a0d9820.bin";

/// The name that the fuzzer saves the single byte 0 under.
const ZERO: &str = "af63bd4c8601b7df.bin";

/// An empty folder of the test's own.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("fuzzer")
        .join(name);
    if folder.exists() {
        fs::remove_dir_all(&folder).expect("the folder of an earlier run can be removed");
    }
    fs::create_dir_all(&folder).expect("the test's folder can be made");
    folder
}

/// A command that runs the fuzzer with `args`.
fn fuzzer(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ferrolua-fuzz"));
    command.args(args).env_remove("RUST_BACKTRACE");
    command
}

fn run(command: &mut Command) -> Output {
    command.output().expect("the fuzzer runs")
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("the report is UTF-8")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("the errors are UTF-8")
}

fn text(path: &Path) -> &str {
    path.to_str().expect("the test's paths are UTF-8")
}

/// The names of the files in `folder`, sorted.
fn files(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("the folder can be read")
        .map(|entry| {
            entry
                .expect("the entry can be read")
                .file_name()
                .into_string()
                .expect("the name is UTF-8")
        })
        .collect();
    names.sort();
    names
}

/// The report of a campaign that found a failure and saved it in `saved`.
fn found(
    target: &str,
    run: u64,
    failure: &str,
    length: usize,
    shrunk: usize,
    saved: &Path,
) -> String {
    format!(
        "{target}: run {run} (seed {run}) {failure}\n  \
         length: {length}, shrunk to {shrunk}\n  \
         saved: {0}\n  \
         replay: cargo xtask fuzz {target} --replay {0}\n",
        saved.display()
    )
}

#[test]
fn finds_the_fixture_panic_from_seed_0_shrinks_and_saves_it_then_replays_it() {
    let folder = folder("fixture-panic");
    let regressions = folder.join("regressions");
    let output = run(&mut fuzzer(&[
        "fixture-panic",
        "--seed",
        "0",
        "--runs",
        "100000",
        "--regressions",
        text(&regressions),
    ]));
    let saved = regressions.join("fixture-panic").join(FUZZ);
    assert_eq!(
        stdout(&output),
        found(
            "fixture-panic",
            676,
            "panicked: the input starts with FUZZ",
            52,
            4,
            &saved
        )
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(files(&regressions.join("fixture-panic")), [FUZZ]);
    assert_eq!(fs::read(&saved).expect("the input is saved"), b"FUZZ");

    let replay = run(&mut fuzzer(&[
        "--replay-saved",
        "--regressions",
        text(&regressions),
    ]));
    assert_eq!(
        stdout(&replay),
        format!(
            "fixture-panic/{FUZZ}: panicked: the input starts with FUZZ\n\
             saved inputs: 1, as expected: 1\n"
        )
    );
    assert_eq!(stderr(&replay), "");
    assert_eq!(replay.status.code(), Some(0));
}

#[test]
fn reports_a_hang_once_an_input_runs_past_its_time_limit() {
    let folder = folder("fixture-hang");
    let regressions = folder.join("regressions");
    let output = run(&mut fuzzer(&[
        "fixture-hang",
        "--time-limit",
        "2000",
        "--regressions",
        text(&regressions),
    ]));
    let saved = regressions.join("fixture-hang").join(ZERO);
    assert_eq!(
        stdout(&output),
        found(
            "fixture-hang",
            1750,
            "ran longer than the time limit of 2000 ms",
            3,
            1,
            &saved
        )
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(&saved).expect("the input is saved"), [0]);
}

#[test]
fn reports_no_failure_when_no_run_fails() {
    // Without its corpus, the fixture starts from an empty input, and no 8 edits make `FUZZ`.
    let folder = folder("no-failure");
    let output = run(&mut fuzzer(&[
        "fixture-panic",
        "--runs",
        "2500",
        "--corpus",
        text(&folder.join("corpus")),
        "--regressions",
        text(&folder.join("regressions")),
    ]));
    assert_eq!(
        stdout(&output),
        "fixture-panic: no run failed (runs: 2500, first seed: 0)\n"
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert!(!folder.join("regressions").exists());
}

#[test]
fn fails_when_the_corpus_cannot_be_read() {
    let folder = folder("corpus-file");
    let corpus = folder.join("corpus");
    fs::write(&corpus, "a file where the folder goes\n").expect("the file can be written");
    let target_corpus = corpus.join("fixture-panic");
    let source = fs::read_dir(&target_corpus).expect_err("a file holds no folder");
    let output = run(&mut fuzzer(&["fixture-panic", "--corpus", text(&corpus)]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: couldn't read the corpus in `{}`: {source}\n",
            target_corpus.display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fails_when_the_failing_input_cannot_be_saved() {
    let folder = folder("regressions-file");
    let regressions = folder.join("regressions");
    fs::write(&regressions, "a file where the folder goes\n").expect("the file can be written");
    let target_regressions = regressions.join("fixture-panic");
    let source = fs::create_dir_all(&target_regressions).expect_err("a file holds no folder");
    let output = run(&mut fuzzer(&[
        "fixture-panic",
        "--regressions",
        text(&regressions),
    ]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: couldn't save the input in `{}`: {source}\n",
            target_regressions.display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn replays_each_saved_input_and_reports_one_that_runs_as_it_should_not() {
    let folder = folder("replay-saved");
    let regressions = folder.join("regressions");
    let write = |path: &str, input: &[u8]| {
        let path = regressions.join(path);
        fs::create_dir_all(path.parent().expect("the input is in a folder"))
            .expect("the folder can be made");
        fs::write(path, input).expect("the input can be written");
    };
    write("fixture-panic/a.bin", b"FUZZ");
    write("fixture-panic/b.bin", b"FUZ");
    write("fixture-hang/c.bin", &[1, 0]);
    let output = run(&mut fuzzer(&[
        "--replay-saved",
        "--regressions",
        text(&regressions),
        "--time-limit",
        "100",
    ]));
    assert_eq!(
        stdout(&output),
        "fixture-hang/c.bin: ran longer than the time limit of 100 ms\n\
         fixture-panic/a.bin: panicked: the input starts with FUZZ\n\
         fixture-panic/b.bin: passed, but it should have panicked\n\
         saved inputs: 3, as expected: 2\n"
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn replays_no_input_when_none_is_saved() {
    let folder = folder("replay-nothing");
    let output = run(&mut fuzzer(&[
        "--replay-saved",
        "--regressions",
        text(&folder.join("regressions")),
    ]));
    assert_eq!(stdout(&output), "saved inputs: 0, as expected: 0\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn fails_when_a_folder_of_saved_inputs_is_named_after_no_target() {
    let folder = folder("replay-unknown");
    let unknown = folder.join("parser");
    fs::create_dir(&unknown).expect("the folder can be made");
    let output = run(&mut fuzzer(&[
        "--replay-saved",
        "--regressions",
        text(&folder),
    ]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: `{}` is named after no fuzz target\n",
            unknown.display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

/// The test that `cargo test` replays every saved input with: each input in
/// `fuzz/regressions/` must run as its target says, which for a target that checks Ferrolua's code
/// means it passes.
#[test]
fn every_input_saved_in_the_repository_runs_as_it_should() {
    let regressions = Path::new(env!("CARGO_MANIFEST_DIR")).join("regressions");
    let count = fs::read_dir(&regressions).map_or(0, |folders| {
        folders
            .map(|folder| files(&folder.expect("the entry can be read").path()).len())
            .sum()
    });
    let output = run(&mut fuzzer(&["--replay-saved"]));
    assert!(
        stdout(&output).ends_with(&format!("saved inputs: {count}, as expected: {count}\n")),
        "{}",
        stdout(&output)
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn replays_an_input_that_passes_in_this_process() {
    let folder = folder("replay-passes");
    let input = folder.join("input.bin");
    fs::write(&input, "FUZ").expect("the input can be written");
    let output = run(&mut fuzzer(&["fixture-panic", "--replay", text(&input)]));
    assert_eq!(
        stdout(&output),
        format!("fixture-panic: {} passed\n", input.display())
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn replays_an_input_that_panics_with_the_panics_location() {
    let folder = folder("replay-panics");
    let input = folder.join("input.bin");
    fs::write(&input, "FUZZ").expect("the input can be written");
    let output = run(&mut fuzzer(&["fixture-panic", "--replay", text(&input)]));
    assert_eq!(stdout(&output), "");
    let location = Path::new("fuzz").join("src").join("target.rs");
    assert!(
        stderr(&output).contains(&format!("panicked at {}:", location.display())),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains(":\nthe input starts with FUZZ\n"),
        "{}",
        stderr(&output)
    );
    assert_eq!(output.status.code(), Some(101));
}

#[test]
fn fails_when_the_input_to_replay_cannot_be_read() {
    let folder = folder("replay-missing");
    let input = folder.join("missing.bin");
    let source = fs::read(&input).expect_err("the input doesn't exist");
    let output = run(&mut fuzzer(&["fixture-panic", "--replay", text(&input)]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: couldn't read the input `{}`: {source}\n",
            input.display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn a_child_fails_on_a_batch_cut_short() {
    let mut child = fuzzer(&["--child", "fixture-panic"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fuzzer starts");
    child
        .stdin
        .take()
        .expect("standard input is piped")
        .write_all(&[1, 0, 0])
        .expect("the batch can be written");
    let output = child.wait_with_output().expect("the child ends");
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        "error: the batch of inputs on standard input is cut short\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn prints_usage_and_exits_with_status_2_without_a_command() {
    let output = run(&mut fuzzer(&[]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        "\
usage: cargo xtask fuzz <target> [--seed <n>] [--runs <n>] [--time-limit <ms>]
                        [--corpus <folder>] [--regressions <folder>]
       cargo xtask fuzz <target> --replay <file>
       cargo xtask fuzz --replay-saved [--regressions <folder>] [--time-limit <ms>]

The fuzzer runs <target> on inputs mutated from its corpus until one panics, crashes, or runs
past the time limit. Then it shrinks that input, saves it, and stops.

  --seed <n>              the seed of the first run; run i uses seed <n> + i (default 0)
  --runs <n>              how many inputs to run (default 100000)
  --time-limit <ms>       how long one input may run (default 1000)
  --corpus <folder>       the folder of each target's corpus folder (default fuzz/corpus)
  --regressions <folder>  the folder of each target's saved inputs (default fuzz/regressions)
  --replay <file>         run <target> on one input, in this process
  --replay-saved          run every saved input, and check that each runs as it should

targets:
  fixture-panic  panics on an input that starts with FUZZ, to test the fuzzer
  fixture-hang   loops forever when its input's bytes link back, to test the fuzzer
"
    );
    assert_eq!(output.status.code(), Some(2));
}
