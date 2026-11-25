//! Runs `cargo xtask record` in small workspaces that each test writes, with a stand-in for
//! PUC-Rio Lua, and checks what it reports and which `.out` files it saves. Each workspace is in a
//! folder of its own under the target folder, so a test never touches Ferrolua's own scripts.
//!
//! The stand-in is a Rust program that the first test to need it compiles with `rustc`. It
//! accepts only the arguments that the task passes, `-E -v` and `-E <script>`, and fails on any
//! others:
//!
//! - For `-E -v`, it prints the value of `FAKE_LUA_VERSION` as its version line, or exits with
//!   status 1 when that variable isn't set.
//! - For `-E <script>`, it prints the script's name, the name of the folder it runs in, and the
//!   script's text. A script whose text starts with `error ` fails instead: the stand-in prints a
//!   message in the form Lua uses to standard error, and exits with status 1.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::OnceLock;

/// The source of the stand-in for PUC-Rio Lua.
const FAKE_LUA: &str = r#"
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [e, v] if e == "-E" && v == "-v" => match env::var("FAKE_LUA_VERSION") {
            Ok(version) => println!("{version}"),
            Err(_) => {
                eprintln!("fake lua: no version");
                process::exit(1);
            }
        },
        [e, script] if e == "-E" => {
            let text = fs::read_to_string(script).expect("the script can be read");
            if let Some(message) = text.strip_prefix("error ") {
                eprint!("lua: {script}:1: {message}");
                process::exit(1);
            }
            let dir = env::current_dir().expect("the current folder can be read");
            let folder = dir.file_name().expect("the folder has a name");
            print!("{script} in {}\n{text}", folder.to_string_lossy());
        }
        _ => {
            eprintln!("fake lua: unexpected arguments {args:?}");
            process::exit(2);
        }
    }
}
"#;

/// The version line that PUC-Rio Lua 5.5.1 prints.
const LUA_5_5_1: &str = "Lua 5.5.1  Copyright (C) 1994-2026 Lua.org, PUC-Rio";

/// How the task tells the user to fix an interpreter it can't use.
const INSTALL_LUA: &str = "install PUC-Rio Lua 5.5.1, or set `FERROLUA_PUC_LUA` to its path";

/// The stand-in for PUC-Rio Lua, named `lua` in a folder of its own, compiled once for all the
/// tests.
fn fake_lua() -> &'static Path {
    static LUA: OnceLock<PathBuf> = OnceLock::new();
    LUA.get_or_init(|| {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("record")
            .join("bin");
        fs::create_dir_all(&dir).expect("the stand-in's folder can be made");
        let source = dir.join("lua.rs");
        fs::write(&source, FAKE_LUA).expect("the stand-in's source can be written");
        let lua = dir.join(format!("lua{}", env::consts::EXE_SUFFIX));
        let status = Command::new("rustc")
            .args(["--edition", "2024", "-o"])
            .arg(&lua)
            .arg(&source)
            .status()
            .expect("`rustc` runs");
        assert!(status.success(), "the stand-in compiles");
        lua
    })
}

/// The status that the stand-in exits with when it fails.
fn status_1() -> ExitStatus {
    let status = Command::new(fake_lua())
        .args(["-E", "-v"])
        .env_remove("FAKE_LUA_VERSION")
        .stderr(Stdio::null())
        .status()
        .expect("the stand-in runs");
    assert_eq!(status.code(), Some(1));
    status
}

/// A workspace that a test writes into a folder of its own.
struct Workspace {
    dir: PathBuf,
}

impl Workspace {
    /// Makes a workspace named `name`, with no members and the files in `files`.
    fn new(name: &str, files: &[(&str, &str)]) -> Workspace {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("record")
            .join(name);
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("the folder of an earlier run can be removed");
        }
        let workspace = Workspace { dir };
        workspace.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = []\n",
        );
        for (path, text) in files {
            workspace.write(path, text);
        }
        workspace
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.dir.join(path);
        fs::create_dir_all(path.parent().expect("a file is in a folder"))
            .expect("the file's folder can be made");
        fs::write(path, text).expect("the file can be written");
    }

    /// The text of the file at `path`.
    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.dir.join(path)).expect("the file can be read")
    }

    fn exists(&self, path: &str) -> bool {
        self.dir.join(path).exists()
    }

    /// The folder of the reference scripts, as the task names it in its errors.
    fn reference(&self) -> PathBuf {
        self.dir.join("tests/conformance/reference")
    }

    /// A `cargo xtask record` command with `args`, run in this workspace with the stand-in as Lua
    /// 5.5.1.
    fn record(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command
            .arg("record")
            .args(args)
            .current_dir(&self.dir)
            .env("FERROLUA_PUC_LUA", fake_lua())
            .env("FAKE_LUA_VERSION", LUA_5_5_1);
        command
    }
}

/// Two reference scripts, the Ferrolua script whose twin is `a.lua`, and a file in the reference
/// folder that isn't a script.
const SCRIPTS: [(&str, &str); 4] = [
    ("tests/conformance/a.lua", "print(\"a\")\n"),
    ("tests/conformance/reference/a.lua", "print(\"a\")\n"),
    ("tests/conformance/reference/b.lua", "print(\"b\")\n"),
    ("tests/conformance/reference/notes.txt", "not a script\n"),
];

/// What the stand-in prints for `a.lua` and `b.lua`.
const A_OUT: &str = "a.lua in reference\nprint(\"a\")\n";
const B_OUT: &str = "b.lua in reference\nprint(\"b\")\n";

fn run(command: &mut Command) -> Output {
    command.output().expect("`cargo xtask` runs")
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("the report is UTF-8")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("the errors are UTF-8")
}

#[test]
fn records_every_reference_script_and_reports_each_out_file() {
    let workspace = Workspace::new("every-script", &SCRIPTS);
    let output = run(&mut workspace.record(&[]));
    assert_eq!(
        stdout(&output),
        "tests/conformance/reference/a.out: new\ntests/conformance/reference/b.out: new\n"
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(workspace.read("tests/conformance/reference/a.out"), A_OUT);
    assert_eq!(workspace.read("tests/conformance/reference/b.out"), B_OUT);
    assert!(!workspace.exists("tests/conformance/a.out"));
    assert!(!workspace.exists("tests/conformance/reference/notes.out"));
}

#[test]
fn records_only_the_script_named_on_the_command_line() {
    let workspace = Workspace::new("one-script", &SCRIPTS);
    let output = run(&mut workspace.record(&["tests/conformance/reference/b.lua"]));
    assert_eq!(stdout(&output), "tests/conformance/reference/b.out: new\n");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(workspace.read("tests/conformance/reference/b.out"), B_OUT);
    assert!(!workspace.exists("tests/conformance/reference/a.out"));
}

#[test]
fn records_the_workspace_s_scripts_from_a_folder_inside_it() {
    let workspace = Workspace::new("inner-folder", &SCRIPTS);
    let output = run(workspace
        .record(&["reference/a.lua"])
        .current_dir(workspace.dir.join("tests/conformance")));
    assert_eq!(stdout(&output), "tests/conformance/reference/a.out: new\n");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(workspace.read("tests/conformance/reference/a.out"), A_OUT);
}

#[test]
fn leaves_an_out_file_unchanged_when_its_script_is_recorded_again() {
    let workspace = Workspace::new("again", &SCRIPTS);
    let first = run(&mut workspace.record(&["tests/conformance/reference/a.lua"]));
    let second = run(&mut workspace.record(&["tests/conformance/reference/a.lua"]));
    assert_eq!(stdout(&first), "tests/conformance/reference/a.out: new\n");
    assert_eq!(
        stdout(&second),
        "tests/conformance/reference/a.out: unchanged\n"
    );
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(workspace.read("tests/conformance/reference/a.out"), A_OUT);
}

#[test]
fn replaces_an_out_file_that_holds_other_output() {
    let workspace = Workspace::new("changed", &SCRIPTS);
    workspace.write("tests/conformance/reference/a.out", "old output\n");
    let output = run(&mut workspace.record(&[]));
    assert_eq!(
        stdout(&output),
        "tests/conformance/reference/a.out: changed\ntests/conformance/reference/b.out: new\n"
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(workspace.read("tests/conformance/reference/a.out"), A_OUT);
}

#[test]
fn finds_lua_on_the_path_when_ferrolua_puc_lua_is_not_set() {
    let workspace = Workspace::new("lua-on-path", &SCRIPTS);
    let bin = fake_lua().parent().expect("the stand-in is in a folder");
    let path = env::var_os("PATH").unwrap_or_default();
    let path = env::join_paths(std::iter::once(bin.to_owned()).chain(env::split_paths(&path)))
        .expect("the folders join into a `PATH`");
    let output = run(workspace
        .record(&["tests/conformance/reference/a.lua"])
        .env_remove("FERROLUA_PUC_LUA")
        .env("PATH", path));
    assert_eq!(stdout(&output), "tests/conformance/reference/a.out: new\n");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(workspace.read("tests/conformance/reference/a.out"), A_OUT);
}

#[test]
fn refuses_an_interpreter_that_reports_another_version() {
    let workspace = Workspace::new("lua-5.4", &SCRIPTS);
    let version = "Lua 5.4.7  Copyright (C) 1994-2024 Lua.org, PUC-Rio";
    let output = run(workspace.record(&[]).env("FAKE_LUA_VERSION", version));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: `{}` reports `{version}` as its version, not Lua 5.5.1: {INSTALL_LUA}\n",
            fake_lua().display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!workspace.exists("tests/conformance/reference/a.out"));
}

#[test]
fn refuses_an_interpreter_that_reports_no_version() {
    let workspace = Workspace::new("no-version", &SCRIPTS);
    let output = run(workspace.record(&[]).env("FAKE_LUA_VERSION", ""));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: `{}` reports no version, not Lua 5.5.1: {INSTALL_LUA}\n",
            fake_lua().display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!workspace.exists("tests/conformance/reference/a.out"));
}

#[test]
fn refuses_an_interpreter_that_fails_when_asked_for_its_version() {
    let workspace = Workspace::new("version-fails", &SCRIPTS);
    let output = run(workspace.record(&[]).env_remove("FAKE_LUA_VERSION"));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "fake lua: no version\n\
             error: `{}` reports no version, not Lua 5.5.1: {INSTALL_LUA}\n",
            fake_lua().display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fails_when_lua_cannot_start() {
    let workspace = Workspace::new("no-lua", &SCRIPTS);
    let lua = workspace.dir.join("no-such-lua");
    let source = Command::new(&lua)
        .output()
        .expect_err("the program doesn't exist");
    let output = run(workspace.record(&[]).env("FERROLUA_PUC_LUA", &lua));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: couldn't start `{}`: {source}: {INSTALL_LUA}\n",
            lua.display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn saves_no_out_file_when_lua_fails_on_a_script() {
    let workspace = Workspace::new("script-fails", &SCRIPTS);
    workspace.write("tests/conformance/reference/b.lua", "error boom\n");
    let output = run(&mut workspace.record(&[]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "lua: b.lua:1: boom\nerror: `{}` failed on `{}`: {}\n",
            fake_lua().display(),
            workspace.reference().join("b.lua").display(),
            status_1()
        )
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!workspace.exists("tests/conformance/reference/a.out"));
    assert!(!workspace.exists("tests/conformance/reference/b.out"));
}

#[test]
fn refuses_a_script_outside_the_reference_folder() {
    let workspace = Workspace::new("outside", &SCRIPTS);
    let output = run(&mut workspace.record(&["tests/conformance/a.lua"]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        "error: `tests/conformance/a.lua` isn't a reference script: name a `.lua` file in \
         `tests/conformance/reference/`\n"
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!workspace.exists("tests/conformance/a.out"));
}

#[test]
fn refuses_a_file_in_the_reference_folder_that_is_not_a_script() {
    let workspace = Workspace::new("not-a-script", &SCRIPTS);
    let output = run(&mut workspace.record(&["tests/conformance/reference/notes.txt"]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        "error: `tests/conformance/reference/notes.txt` isn't a reference script: name a `.lua` \
         file in `tests/conformance/reference/`\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fails_when_the_named_script_does_not_exist() {
    let workspace = Workspace::new("missing-script", &SCRIPTS);
    let script = "tests/conformance/reference/missing.lua";
    let source = fs::canonicalize(workspace.dir.join(script)).expect_err("the script is missing");
    let output = run(&mut workspace.record(&[script]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!("error: couldn't read `{script}`: {source}\n")
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fails_when_the_reference_folder_holds_no_scripts() {
    let workspace = Workspace::new(
        "no-scripts",
        &[("tests/conformance/reference/notes.txt", "not a script\n")],
    );
    let output = run(&mut workspace.record(&[]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: `{}` holds no `.lua` scripts\n",
            workspace.reference().display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fails_when_the_reference_folder_does_not_exist() {
    let workspace = Workspace::new("no-folder", &[]);
    let source = fs::read_dir(workspace.reference()).expect_err("the folder is missing");
    let output = run(&mut workspace.record(&[]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: couldn't read `{}`: {source}\n",
            workspace.reference().display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fails_when_an_out_file_cannot_be_saved() {
    let workspace = Workspace::new("cannot-save", &SCRIPTS);
    let out = workspace.reference().join("a.out");
    fs::create_dir(&out).expect("a folder can take the `.out` file's name");
    let source = fs::read(&out).expect_err("a folder can't be read as a file");
    let output = run(&mut workspace.record(&["tests/conformance/reference/a.lua"]));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!("error: couldn't save `{}`: {source}\n", out.display())
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn finds_cargo_on_the_path_when_cargo_is_not_set() {
    let workspace = Workspace::new("cargo-on-path", &SCRIPTS);
    let output = run(workspace
        .record(&["tests/conformance/reference/a.lua"])
        .env_remove("CARGO"));
    assert_eq!(stdout(&output), "tests/conformance/reference/a.out: new\n");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(workspace.read("tests/conformance/reference/a.out"), A_OUT);
}

#[test]
fn fails_when_cargo_cannot_start() {
    let workspace = Workspace::new("no-cargo", &SCRIPTS);
    let cargo = workspace.dir.join("no-such-cargo");
    let source = Command::new(&cargo)
        .output()
        .expect_err("the program doesn't exist");
    let output = run(workspace.record(&[]).env("CARGO", &cargo));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!("error: couldn't start `cargo metadata`: {source}\n")
    );
    assert_eq!(output.status.code(), Some(1));
}
