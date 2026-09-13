//! `diag`: run cargo with JSON diagnostics and print a compact report.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;

use clap::ValueEnum;
use serde_json::Value;

#[derive(Clone, Copy, ValueEnum)]
pub enum Sub {
    Check,
    Clippy,
    Build,
}

impl Sub {
    fn as_str(self) -> &'static str {
        match self {
            Sub::Check => "check",
            Sub::Clippy => "clippy",
            Sub::Build => "build",
        }
    }
}

/// Cargo status verbs that carry no diagnostic information.
const STATUS_VERBS: &[&str] = &[
    "Compiling",
    "Checking",
    "Fresh",
    "Finished",
    "Blocking",
    "Downloading",
    "Downloaded",
    "Locking",
    "Updating",
    "Adding",
];

#[derive(Default)]
pub struct Report {
    passthrough: Vec<String>,
    errors: Vec<String>,
    warnings: Vec<String>,
    seen: HashSet<String>,
    duplicates: usize,
}

impl Report {
    /// Feed one line of cargo stdout.
    pub fn stdout_line(&mut self, line: &str, full_warnings: bool) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            self.passthrough.push(line.to_string());
            return;
        };
        if value["reason"] != "compiler-message" {
            return;
        }
        let msg = &value["message"];
        let level = msg["level"].as_str().unwrap_or("");
        let text = msg["message"].as_str().unwrap_or("");
        let no_spans = msg["spans"].as_array().is_none_or(|s| s.is_empty());
        if no_spans && (text.starts_with("aborting due to") || text.ends_with("emitted")) {
            return;
        }
        let rendered = msg["rendered"]
            .as_str()
            .unwrap_or(text)
            .trim_end()
            .to_string();
        if level.starts_with("error") {
            self.add(rendered, true);
        } else if level == "warning" {
            let entry = if full_warnings {
                rendered
            } else {
                one_line(msg, text)
            };
            self.add(entry, false);
        }
    }

    /// Feed one line of cargo stderr.
    pub fn stderr_line(&mut self, line: &str) {
        let first = line.split_whitespace().next().unwrap_or("");
        if !STATUS_VERBS.contains(&first) {
            self.passthrough.push(line.to_string());
        }
    }

    fn add(&mut self, entry: String, is_error: bool) {
        if !self.seen.insert(entry.clone()) {
            self.duplicates += 1;
            return;
        }
        if is_error {
            self.errors.push(entry);
        } else {
            self.warnings.push(entry);
        }
    }

    pub fn render(&self, max_errors: usize) -> String {
        let mut out = String::new();
        let shown = if max_errors == 0 {
            self.errors.len()
        } else {
            max_errors.min(self.errors.len())
        };
        for err in &self.errors[..shown] {
            out.push_str(err);
            out.push_str("\n\n");
        }
        if shown < self.errors.len() {
            let hidden = self.errors.len() - shown;
            let noun = if hidden == 1 { "error" } else { "errors" };
            out.push_str(&format!("… +{hidden} more {noun} (use --max-errors 0)\n\n"));
        }
        for warning in &self.warnings {
            out.push_str(warning);
            out.push('\n');
        }
        for line in &self.passthrough {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!(
            "{}, {} ({} duplicates merged)\n",
            plural(self.errors.len(), "error"),
            plural(self.warnings.len(), "warning"),
            self.duplicates
        ));
        out
    }
}

fn one_line(msg: &Value, text: &str) -> String {
    let span = msg["spans"]
        .as_array()
        .and_then(|spans| spans.iter().find(|s| s["is_primary"] == true));
    let code = msg["code"]["code"]
        .as_str()
        .map(|c| format!("[{c}]"))
        .unwrap_or_default();
    match span {
        Some(s) => format!(
            "{}:{}:{}: warning{code}: {text}",
            s["file_name"].as_str().unwrap_or("?"),
            s["line_start"],
            s["column_start"]
        ),
        None => format!("warning{code}: {text}"),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

pub fn run(
    sub: Sub,
    max_errors: usize,
    full_warnings: bool,
    cargo_args: &[String],
) -> anyhow::Result<u8> {
    let mut child = Command::new("cargo")
        .arg(sub.as_str())
        .arg("--message-format=json")
        .args(cargo_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stderr = child.stderr.take().expect("piped stderr");
    let stderr_thread = thread::spawn(move || {
        BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
            .collect::<Vec<_>>()
    });
    let mut report = Report::default();
    let stdout = child.stdout.take().expect("piped stdout");
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        report.stdout_line(&line, full_warnings);
    }
    for line in stderr_thread.join().unwrap_or_default() {
        report.stderr_line(&line);
    }
    let status = child.wait()?;
    print!("{}", report.render(max_errors));
    Ok(status.code().map_or(1, |c| u8::try_from(c).unwrap_or(1)))
}
