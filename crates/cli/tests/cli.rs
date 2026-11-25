//! Runs the `lunaferra` command's binary the way a user would.

use std::process::Command;

#[test]
fn exits_with_status_0_and_prints_nothing() {
    // The binary has no commands yet; task M1.10 adds `lunaferra parse`.
    let output = Command::new(env!("CARGO_BIN_EXE_lunaferra-cli"))
        .output()
        .expect("the binary runs");
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}
