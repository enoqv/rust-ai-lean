mod common;

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
