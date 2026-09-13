use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn read_json(rel: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

#[test]
fn versions_match_across_cargo_plugin_and_marketplace() {
    let cargo = env!("CARGO_PKG_VERSION");
    let plugin = read_json("plugin/.claude-plugin/plugin.json");
    let market = read_json(".claude-plugin/marketplace.json");
    assert_eq!(plugin["version"], cargo, "plugin.json version");
    let entry = market["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "rust-ai-lean")
        .expect("marketplace entry");
    assert_eq!(entry["version"], cargo, "marketplace.json version");
}

fn launcher_script() -> String {
    let plugin = read_json("plugin/.claude-plugin/plugin.json");
    let server = &plugin["lspServers"]["rust-analyzer"];
    assert_eq!(server["command"], "sh");
    assert_eq!(server["args"][0], "-c");
    server["args"][1].as_str().unwrap().to_string()
}

fn stub(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn launch(path: &str) -> (Option<i32>, String) {
    let out = Command::new("/bin/sh")
        .args(["-c", &launcher_script(), "ra-launch"])
        .env_clear()
        .env("PATH", path)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn launcher_prefers_rustup_stable_rust_analyzer() {
    let tmp = tempfile::tempdir().unwrap();
    let ra = tmp.path().join("toolchain");
    stub(&ra, "rust-analyzer", "echo stable-ra");
    let bin = tmp.path().join("bin");
    stub(
        &bin,
        "rustup",
        &format!(
            "[ \"$*\" = 'which --toolchain stable rust-analyzer' ] && echo {}/rust-analyzer",
            ra.display()
        ),
    );
    stub(&bin, "rust-analyzer", "echo path-ra");
    let (code, out) = launch(&format!("{}:/usr/bin:/bin", bin.display()));
    assert_eq!((code, out.trim()), (Some(0), "stable-ra"));
}

#[test]
fn launcher_fails_when_rustup_lacks_component() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    stub(&bin, "rustup", "exit 1");
    stub(&bin, "rust-analyzer", "echo proxy-must-not-run");
    let (code, out) = launch(&format!("{}:/usr/bin:/bin", bin.display()));
    assert_eq!(code, Some(1));
    assert!(!out.contains("proxy-must-not-run"));
}

#[test]
fn launcher_uses_path_without_rustup() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    stub(&bin, "rust-analyzer", "echo path-ra");
    let (code, out) = launch(&format!("{}:/usr/bin:/bin", bin.display()));
    assert_eq!((code, out.trim()), (Some(0), "path-ra"));
}
