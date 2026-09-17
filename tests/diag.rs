mod common;

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use common::{fixture, run_in, stdout};

#[test]
fn errors_survive_many_warnings_and_come_first() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("diag-noisy"),
        tmp.path(),
        &["diag", "check", "--all-targets"],
    );
    assert_eq!(out.status.code(), Some(101));
    let text = stdout(&out);
    assert!(
        text.starts_with("error[E0425]: cannot find value `undefined_value`"),
        "{text}"
    );
    assert!(text.contains("error[E0308]: mismatched types"), "{text}");
    assert!(
        text.contains("expected `u32` because of return type"),
        "help/label lines kept:\n{text}"
    );
    assert_eq!(
        text.matches(": warning[unused_variables]: unused variable")
            .count(),
        25,
        "{text}"
    );
    assert!(
        text.ends_with("2 errors, 25 warnings (27 duplicates merged)\n"),
        "{text}"
    );
    for status in ["Checking", "Compiling", "Finished"] {
        assert!(
            !text.lines().any(|l| l.trim_start().starts_with(status)),
            "{status} leaked:\n{text}"
        );
    }
}

#[test]
fn max_errors_truncates_with_notice() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("diag-noisy"),
        tmp.path(),
        &["diag", "check", "--max-errors", "1"],
    );
    let text = stdout(&out);
    assert_eq!(
        text.matches("\nerror[E").count() + usize::from(text.starts_with("error[E")),
        1,
        "{text}"
    );
    assert!(
        text.contains("… +1 more error (use --max-errors 0)"),
        "{text}"
    );
}

#[test]
fn full_warnings_render_complete_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("diag-noisy"),
        tmp.path(),
        &["diag", "check", "--full-warnings"],
    );
    let text = stdout(&out);
    assert!(
        text.contains("warning: unused variable: `value_00`\n --> src/lib.rs:1:26"),
        "{text}"
    );
    assert!(
        text.contains("help: if this is intentional, prefix it with an underscore"),
        "{text}"
    );
}

#[test]
fn non_json_cargo_errors_pass_through() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("diag-buildrs"), tmp.path(), &["diag", "build"]);
    assert_eq!(out.status.code(), Some(101));
    let text = stdout(&out);
    assert!(
        text.contains("failed to run custom build command"),
        "{text}"
    );
    assert!(
        text.contains("BUILD-SCRIPT-MARKER: generator failed"),
        "{text}"
    );
}

#[test]
fn own_flags_are_honored_after_cargo_args() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("diag-noisy"),
        tmp.path(),
        &["diag", "check", "--all-targets", "--max-errors", "1"],
    );
    let text = stdout(&out);
    assert!(
        text.contains("… +1 more error (use --max-errors 0)"),
        "{text}"
    );
    assert!(text.ends_with("(27 duplicates merged)\n"), "{text}");
}

/// Runs `diag` with a stub `cargo` whose body is `script`.
fn run_with_stub_cargo(script: &str, args: &[&str]) -> std::process::Output {
    let tmp = tempfile::tempdir().unwrap();
    let cargo = tmp.path().join("cargo");
    std::fs::write(&cargo, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rust-ai-lean"))
        .args(args)
        .env("PATH", format!("{}:/usr/bin:/bin", tmp.path().display()))
        .output()
        .unwrap()
}

#[test]
fn double_dash_and_everything_after_it_reach_cargo() {
    let out = run_with_stub_cargo(
        "echo \"ARGS:$*\"",
        &[
            "diag",
            "clippy",
            "-p",
            "x",
            "--",
            "-D",
            "warnings",
            "--max-errors",
            "2",
        ],
    );
    let text = stdout(&out);
    assert!(
        text.contains("ARGS:clippy --message-format=json -p x -- -D warnings --max-errors 2"),
        "{text}"
    );
    assert!(
        text.ends_with("0 errors, 0 warnings (0 duplicates merged)\n"),
        "{text}"
    );
}

#[test]
fn signal_termination_exits_128_plus_signal() {
    let out = run_with_stub_cargo("kill -TERM $$", &["diag", "check"]);
    assert_eq!(out.status.code(), Some(128 + 15));
}

#[test]
fn cargo_color_is_forced_off() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rust-ai-lean"))
        .args(["diag", "check"])
        .current_dir(fixture("diag-noisy"))
        .env("CARGO_TARGET_DIR", tmp.path())
        .env("CARGO_TERM_COLOR", "always")
        .output()
        .unwrap();
    let text = stdout(&out);
    assert!(!text.contains("\u{1b}["), "{text}");
    assert!(
        !text.lines().any(|l| l.trim_start().starts_with("Checking")),
        "{text}"
    );
}
