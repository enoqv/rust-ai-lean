#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Run the CLI in `dir` with an isolated cargo target directory.
pub fn run_in(dir: &Path, target: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rust-ai-lean"))
        .args(args)
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target)
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("run rust-ai-lean")
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}
