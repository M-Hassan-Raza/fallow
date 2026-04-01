#[path = "common/mod.rs"]
mod common;

use common::run_fallow_raw;
use std::fs;

/// Create a unique temp dir for init tests.
fn init_temp_dir(suffix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fallow-init-test-{}-{}",
        std::process::id(),
        suffix
    ));
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    fs::create_dir_all(&dir).unwrap();
    // init requires a package.json to exist
    fs::write(
        dir.join("package.json"),
        r#"{"name": "init-test", "main": "index.ts"}"#,
    )
    .unwrap();
    dir
}

/// Clean up a temp dir after a test.
fn cleanup(dir: &std::path::Path) {
    let _ = fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Init creates config files
// ---------------------------------------------------------------------------

#[test]
fn init_creates_fallowrc_json() {
    let dir = init_temp_dir("json");
    let output = run_fallow_raw(&["init", "--root", dir.to_str().unwrap(), "--quiet"]);
    assert_eq!(
        output.code, 0,
        "init should succeed, stderr: {}",
        output.stderr
    );
    assert!(
        dir.join(".fallowrc.json").exists(),
        "init should create .fallowrc.json"
    );
    cleanup(&dir);
}

#[test]
fn init_creates_toml_with_flag() {
    let dir = init_temp_dir("toml");
    let output = run_fallow_raw(&["init", "--toml", "--root", dir.to_str().unwrap(), "--quiet"]);
    assert_eq!(
        output.code, 0,
        "init --toml should succeed, stderr: {}",
        output.stderr
    );
    assert!(
        dir.join("fallow.toml").exists(),
        "init --toml should create fallow.toml"
    );
    cleanup(&dir);
}

#[test]
fn init_exits_nonzero_if_config_exists() {
    let dir = init_temp_dir("exists");
    run_fallow_raw(&["init", "--root", dir.to_str().unwrap(), "--quiet"]);
    let output = run_fallow_raw(&["init", "--root", dir.to_str().unwrap(), "--quiet"]);
    assert_ne!(
        output.code, 0,
        "init should fail when config already exists"
    );
    cleanup(&dir);
}

#[test]
fn init_created_config_is_valid_json() {
    let dir = init_temp_dir("valid");
    run_fallow_raw(&["init", "--root", dir.to_str().unwrap(), "--quiet"]);
    let content = fs::read_to_string(dir.join(".fallowrc.json")).unwrap();
    // Init generates JSONC (with comments). Strip single-line comments before parsing.
    let stripped: String = content
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let _: serde_json::Value = serde_json::from_str(&stripped)
        .unwrap_or_else(|e| panic!("init should produce valid JSON: {e}\ncontent: {content}"));
    cleanup(&dir);
}
