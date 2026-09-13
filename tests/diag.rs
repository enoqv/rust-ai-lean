mod common;

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
