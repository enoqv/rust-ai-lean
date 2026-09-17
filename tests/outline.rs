mod common;

use std::io::Read;
use std::process::{Command, Stdio};

use common::{fixture, run_in, stderr, stdout};

fn outline(args: &[&str]) -> std::process::Output {
    let tmp = tempfile::tempdir().unwrap();
    run_in(&fixture("outline"), tmp.path(), args)
}

#[test]
fn renders_signatures_with_line_ranges() {
    let out = outline(&["outline", "sample.rs"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    for line in [
        "sample.rs",
        "  6-6       pub const MAX_RETRIES: u32",
        "  14-19     pub struct User { pub id: String, name: String }",
        "  21-21     pub struct Id(pub String)",
        "  25-29     pub enum Error { NotFound, Invalid(..), Conflict { .. } }",
        "  36-43     pub trait Repository: Send + Sync",
        "  39-39       fn get(&self, id: &str) -> Option<Self::Item>",
        "  45-54     impl<T> Repository for Vec<T> where T: Clone + Send + Sync",
        "  60-71       pub async fn insert_user(conn: &mut Vec<User>, id: &str, name: &str, display_name: Option<&str>, email: Option<&str>) -> Result<User>",
        "  84-84         pub(crate) fn inner()",
        "  88-88     mod external",
        "  90-94     macro_rules! square",
    ] {
        assert!(
            text.lines().any(|l| l == line),
            "missing line {line:?} in:\n{text}"
        );
    }
    assert!(
        !text.contains("conn.push"),
        "bodies must not be printed:\n{text}"
    );
    assert!(
        !text.contains("derive"),
        "attributes must not be printed:\n{text}"
    );
}

#[test]
fn collapses_cfg_test_modules() {
    let text = stdout(&outline(&["outline", "sample.rs"]));
    assert!(
        text.contains("  97-103    #[cfg(test)] mod tests  (2 items)\n"),
        "{text}"
    );
    assert!(!text.contains("fn one"), "{text}");
}

#[test]
fn docs_flag_adds_first_doc_line() {
    let text = stdout(&outline(&["outline", "--docs", "sample.rs"]));
    assert!(
        text.contains(
            "  6-6       pub const MAX_RETRIES: u32\n              /// Maximum retries.\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "  60-71       pub async fn insert_user(conn: &mut Vec<User>, id: &str, name: &str, display_name: Option<&str>, email: Option<&str>) -> Result<User>\n                /// Creates a user.\n"
        ),
        "{text}"
    );
    let plain = stdout(&outline(&["outline", "sample.rs"]));
    assert!(!plain.contains("///"), "{plain}");
}

#[test]
fn unparsable_file_gets_approximate_outline() {
    let out = outline(&["outline", "broken.rs"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.starts_with("broken.rs: parse error at 5:"), "{text}");
    assert!(text.contains("approximate outline"), "{text}");
    assert!(
        text.contains("  5-5       pub fn broken(x: u32 -> u32\n"),
        "{text}"
    );
    assert!(
        text.contains("  10-10       pub(crate) fn method(&self)\n"),
        "{text}"
    );
}

#[test]
fn directories_are_walked_skipping_target_and_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    for (path, body) in [
        ("src/a.rs", "pub fn a() {}"),
        ("target/b.rs", "pub fn b() {}"),
        (".hidden/c.rs", "pub fn c() {}"),
    ] {
        let full = tmp.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, body).unwrap();
    }
    let out = run_in(tmp.path(), tmp.path(), &["outline", "."]);
    let text = stdout(&out);
    assert!(text.contains("pub fn a()"), "{text}");
    assert!(
        !text.contains("pub fn b()") && !text.contains("pub fn c()"),
        "{text}"
    );
}

#[test]
fn unreadable_path_exits_1_but_prints_the_rest() {
    let out = outline(&["outline", "missing.rs", "sample.rs"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("missing.rs"));
    assert!(stdout(&out).contains("pub struct Marker"));
}

#[test]
fn broken_pipe_stops_quietly() {
    let tmp = tempfile::tempdir().unwrap();
    let mut args = vec!["outline".to_string()];
    args.extend(std::iter::repeat_n("sample.rs".to_string(), 300));
    let mut child = Command::new(env!("CARGO_BIN_EXE_rust-ai-lean"))
        .args(&args)
        .current_dir(fixture("outline"))
        .env("CARGO_TARGET_DIR", tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut buf = [0u8; 100];
    let mut child_stdout = child.stdout.take().unwrap();
    let _ = child_stdout.read(&mut buf);
    drop(child_stdout);
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(!stderr(&out).contains("Broken pipe"), "{}", stderr(&out));
}
