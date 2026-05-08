#[path = "common/mod.rs"]
mod common;

use common::run_fallow_raw;
use std::fs;

/// Create a temp dir with a knip config for migration testing.
fn migrate_temp_dir(suffix: &str, config_name: &str, config_content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fallow-migrate-test-{}-{}",
        std::process::id(),
        suffix
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("package.json"),
        r#"{"name": "migrate-test", "main": "src/index.ts"}"#,
    )
    .unwrap();
    fs::write(dir.join(config_name), config_content).unwrap();
    dir
}

fn cleanup(dir: &std::path::Path) {
    let _ = fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Migrate dry-run
// ---------------------------------------------------------------------------

#[test]
fn migrate_dry_run_outputs_config() {
    let dir = migrate_temp_dir(
        "dryrun",
        "knip.json",
        r#"{"entry": ["src/index.ts"], "ignore": ["dist/**"]}"#,
    );
    let output = run_fallow_raw(&[
        "migrate",
        "--dry-run",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(
        output.code, 0,
        "migrate --dry-run should exit 0, stderr: {}",
        output.stderr
    );
    // Dry-run prints the generated config to stdout
    assert!(
        output.stdout.contains("entry") || output.stdout.contains("$schema"),
        "dry-run should output the migrated config"
    );
    cleanup(&dir);
}

#[test]
fn migrate_dry_run_toml_output() {
    let dir = migrate_temp_dir("toml", "knip.json", r#"{"entry": ["src/index.ts"]}"#);
    let output = run_fallow_raw(&[
        "migrate",
        "--dry-run",
        "--toml",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(output.code, 0, "migrate --dry-run --toml should exit 0");
    // TOML output should use = syntax
    assert!(
        output.stdout.contains('='),
        "TOML output should use = syntax"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// Output filename selection (--toml / --jsonc / auto-mirror)
// ---------------------------------------------------------------------------

#[test]
fn migrate_writes_fallowrc_json_when_source_is_knip_json() {
    let dir = migrate_temp_dir("out-json", "knip.json", r#"{"entry": ["src/index.ts"]}"#);
    let output = run_fallow_raw(&["migrate", "--root", dir.to_str().unwrap(), "--quiet"]);
    assert_eq!(output.code, 0, "stderr: {}", output.stderr);
    assert!(
        dir.join(".fallowrc.json").exists(),
        ".fallowrc.json should be written for knip.json source"
    );
    assert!(
        !dir.join(".fallowrc.jsonc").exists(),
        ".fallowrc.jsonc should NOT be written for knip.json source"
    );
    cleanup(&dir);
}

#[test]
fn migrate_auto_writes_fallowrc_jsonc_when_source_is_knip_jsonc() {
    let dir = migrate_temp_dir(
        "out-jsonc-auto",
        "knip.jsonc",
        "{\n  // header comment\n  \"entry\": [\"src/index.ts\"]\n}\n",
    );
    let output = run_fallow_raw(&["migrate", "--root", dir.to_str().unwrap(), "--quiet"]);
    assert_eq!(output.code, 0, "stderr: {}", output.stderr);
    assert!(
        dir.join(".fallowrc.jsonc").exists(),
        ".fallowrc.jsonc should be written when source is knip.jsonc"
    );
    assert!(
        !dir.join(".fallowrc.json").exists(),
        ".fallowrc.json should NOT be written when source is knip.jsonc"
    );
    cleanup(&dir);
}

#[test]
fn migrate_explicit_jsonc_flag_overrides_json_source() {
    let dir = migrate_temp_dir(
        "out-jsonc-flag",
        "knip.json",
        r#"{"entry": ["src/index.ts"]}"#,
    );
    let output = run_fallow_raw(&[
        "migrate",
        "--jsonc",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(output.code, 0, "stderr: {}", output.stderr);
    assert!(
        dir.join(".fallowrc.jsonc").exists(),
        "--jsonc must force .fallowrc.jsonc even when source is knip.json"
    );
    assert!(!dir.join(".fallowrc.json").exists());
    cleanup(&dir);
}

#[test]
fn migrate_jsonc_and_toml_are_mutually_exclusive() {
    let dir = migrate_temp_dir("exclusive", "knip.json", r#"{"entry": ["src/index.ts"]}"#);
    let output = run_fallow_raw(&[
        "migrate",
        "--jsonc",
        "--toml",
        "--dry-run",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_ne!(
        output.code, 0,
        "clap should reject --jsonc and --toml together"
    );
    assert!(
        output.stderr.contains("cannot be used with") || output.stderr.contains("conflicts"),
        "expected clap conflict error, got stderr: {}",
        output.stderr
    );
    cleanup(&dir);
}

#[test]
fn migrate_existing_fallowrc_jsonc_blocks_run() {
    let dir = migrate_temp_dir(
        "blocked-jsonc",
        "knip.json",
        r#"{"entry": ["src/index.ts"]}"#,
    );
    fs::write(dir.join(".fallowrc.jsonc"), "{}").unwrap();
    let output = run_fallow_raw(&["migrate", "--root", dir.to_str().unwrap(), "--quiet"]);
    assert_eq!(
        output.code, 2,
        "migrate should refuse to overwrite existing .fallowrc.jsonc"
    );
    assert!(
        output.stderr.contains(".fallowrc.jsonc already exists"),
        "stderr should mention the blocking file, got: {}",
        output.stderr
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// Migrate error handling
// ---------------------------------------------------------------------------

#[test]
fn migrate_no_config_exits_2() {
    let dir = std::env::temp_dir().join(format!("fallow-migrate-noconfig-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("package.json"), r#"{"name": "no-config"}"#).unwrap();

    let output = run_fallow_raw(&[
        "migrate",
        "--dry-run",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(
        output.code, 2,
        "migrate with no source config should exit 2"
    );
    let _ = fs::remove_dir_all(&dir);
}
