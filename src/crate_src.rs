//! `crate-src`: dependency source location for the exact locked version.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Metadata {
    pub packages: Vec<Package>,
    pub resolve: Option<Resolve>,
}

#[derive(Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub manifest_path: PathBuf,
}

#[derive(Deserialize)]
pub struct Resolve {
    pub nodes: Vec<Node>,
}

#[derive(Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub features: Vec<String>,
}

pub fn run(krate: &str, manifest_path: Option<&Path>, path_only: bool) -> anyhow::Result<u8> {
    let mut cmd = Command::new("cargo");
    cmd.args(["metadata", "--format-version", "1", "--locked"]);
    if let Some(path) = manifest_path {
        cmd.arg("--manifest-path").arg(path);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprint!("{stderr}");
        if stderr.contains("could not find `Cargo.toml`") {
            return Ok(2);
        }
        if stderr.contains("--locked") {
            eprintln!("hint: Cargo.lock is missing or out of date; crate-src never modifies it");
        }
        return Ok(1);
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    let (text, code) = render(&metadata, krate, path_only);
    if code == 0 {
        print!("{text}");
    } else {
        eprint!("{text}");
    }
    Ok(code)
}

fn normalize(name: &str) -> String {
    name.replace('-', "_")
}

/// Render matches for `krate`; returns the text and the exit code.
pub fn render(metadata: &Metadata, krate: &str, path_only: bool) -> (String, u8) {
    let wanted = normalize(krate);
    let features: HashMap<&str, &[String]> = metadata
        .resolve
        .iter()
        .flat_map(|r| &r.nodes)
        .map(|n| (n.id.as_str(), n.features.as_slice()))
        .collect();
    let mut matches: Vec<&Package> = metadata
        .packages
        .iter()
        .filter(|p| normalize(&p.name) == wanted)
        .collect();
    matches.sort_by(|a, b| a.version.cmp(&b.version).then(a.id.cmp(&b.id)));
    if matches.is_empty() {
        let similar: BTreeSet<&str> = metadata
            .packages
            .iter()
            .filter(|p| normalize(&p.name).contains(&wanted))
            .map(|p| p.name.as_str())
            .collect();
        let mut text = format!("error: no package named `{krate}` in the dependency graph\n");
        if !similar.is_empty() {
            let list: Vec<&str> = similar.into_iter().take(5).collect();
            text.push_str(&format!("similar: {}\n", list.join(", ")));
        }
        return (text, 1);
    }
    let mut text = String::new();
    for pkg in matches {
        let root = pkg.manifest_path.parent().unwrap_or(Path::new(""));
        if path_only {
            text.push_str(&format!("{}\n", root.display()));
            continue;
        }
        let mut feats: Vec<&str> = features
            .get(pkg.id.as_str())
            .map(|f| f.iter().map(String::as_str).collect())
            .unwrap_or_default();
        feats.sort_unstable();
        let source = pkg.source.as_deref().unwrap_or("path");
        text.push_str(&format!(
            "{} {} ({}) features=[{}]\n  {}\n",
            pkg.name,
            pkg.version,
            source,
            feats.join(","),
            root.display()
        ));
    }
    (text, 0)
}
