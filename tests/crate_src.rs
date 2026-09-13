mod common;

use common::{fixture, run_in, stderr, stdout};

#[test]
fn resolves_registry_dependency_to_locked_version() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("deps"), tmp.path(), &["crate-src", "itoa"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.starts_with("itoa 1.0.18 (registry+"), "{text}");
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .trim_start()
            .ends_with("itoa-1.0.18"),
        "{text}"
    );
}

#[test]
fn matches_path_dependency_ignoring_hyphen_underscore() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("deps"), tmp.path(), &["crate-src", "local_helper"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).starts_with("local-helper 0.2.0 (path) features=[]\n"));
}

#[test]
fn path_only_prints_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        &fixture("deps"),
        tmp.path(),
        &["crate-src", "itoa", "--path-only"],
    );
    let text = stdout(&out);
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.trim_end().ends_with("itoa-1.0.18"), "{text}");
}

#[test]
fn unknown_crate_exits_1_with_suggestions() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(&fixture("deps"), tmp.path(), &["crate-src", "ito"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("no package named `ito`"), "{err}");
    assert!(err.contains("similar: itoa"), "{err}");
}

#[test]
fn no_manifest_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(tmp.path(), tmp.path(), &["crate-src", "itoa"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

#[test]
fn missing_manifest_path_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("nope/Cargo.toml");
    let out = run_in(
        &fixture("deps"),
        tmp.path(),
        &[
            "crate-src",
            "itoa",
            "--manifest-path",
            missing.to_str().unwrap(),
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("does not exist"), "{}", stderr(&out));
}

#[test]
fn stale_lock_exits_1_and_leaves_lock_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("deps");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("local-helper/src")).unwrap();
    for file in [
        "Cargo.lock",
        "src/lib.rs",
        "local-helper/Cargo.toml",
        "local-helper/src/lib.rs",
    ] {
        std::fs::copy(fixture("deps").join(file), dir.join(file)).unwrap();
    }
    let manifest = std::fs::read_to_string(fixture("deps").join("Cargo.toml")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        manifest.replace("=1.0.18", "=1.0.17"),
    )
    .unwrap();
    let lock_before = std::fs::read(dir.join("Cargo.lock")).unwrap();
    let out = run_in(&dir, tmp.path(), &["crate-src", "itoa"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("crate-src never modifies it"));
    assert_eq!(std::fs::read(dir.join("Cargo.lock")).unwrap(), lock_before);
}
