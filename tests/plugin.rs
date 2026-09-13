use std::fs;
use std::path::Path;

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
