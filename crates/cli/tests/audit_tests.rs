#[path = "common/mod.rs"]
mod common;

use common::{parse_json, run_fallow_raw};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .expect("git command failed");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit_all(dir: &std::path::Path, message: &str) {
    git(dir, &["add", "."]);
    git(
        dir,
        &["-c", "commit.gpgsign=false", "commit", "-m", message],
    );
}

/// Create a temp git repo with a commit, suitable for audit testing.
/// Returns the `TempDir` guard so the directory lives as long as the caller holds it.
fn create_audit_fixture(_suffix: &str) -> TempDir {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let dir = tmp.path();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r#"{"name": "audit-test", "main": "src/index.ts", "dependencies": {"unused-pkg": "1.0.0"}}"#,
    )
    .unwrap();

    fs::write(
        dir.join("src/index.ts"),
        "import { used } from './utils';\nused();\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/utils.ts"),
        "export const used = () => 42;\nexport const unused = () => 0;\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/orphan.ts"),
        "export const orphaned = 'nobody';\n",
    )
    .unwrap();

    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            // Isolate from parent git context (pre-push hook sets GIT_DIR to the main repo,
            // which overrides current_dir and causes commits to leak into the real repo)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git command failed")
    };

    git(&["init", "-b", "main"]);
    git(&["add", "."]);
    git(&["-c", "commit.gpgsign=false", "commit", "-m", "initial"]);

    tmp
}

// ---------------------------------------------------------------------------
// Audit JSON output structure
// ---------------------------------------------------------------------------

#[test]
fn audit_json_has_verdict_and_schema() {
    let dir = create_audit_fixture("verdict");
    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.path().to_str().unwrap(),
        "--base",
        "HEAD",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 0,
        "audit with no changes should exit 0. stderr: {}",
        output.stderr
    );

    let json = parse_json(&output);
    assert_eq!(
        json["verdict"].as_str(),
        Some("pass"),
        "no changes should give pass verdict"
    );
    assert_eq!(
        json["command"].as_str(),
        Some("audit"),
        "command should be 'audit'"
    );
    assert!(
        json.get("schema_version").is_some(),
        "audit JSON should have schema_version"
    );
}

#[test]
fn audit_pass_verdict_when_no_changes() {
    let dir = create_audit_fixture("nochanges");
    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.path().to_str().unwrap(),
        "--base",
        "HEAD",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(output.code, 0, "no changes should give exit 0");

    let json = parse_json(&output);
    assert_eq!(
        json["verdict"].as_str(),
        Some("pass"),
        "no changes should give pass verdict"
    );
    assert_eq!(
        json["changed_files_count"].as_u64(),
        Some(0),
        "should report 0 changed files"
    );
}

/// Audit's HEAD analyses and base-snapshot computation run concurrently via
/// `rayon::join`; inside the base snapshot, check and dupes also run
/// concurrently. Verify nondeterministic scheduling does not leak into the
/// rendered JSON: repeated runs against the same fixture must produce
/// byte-identical output once wall-clock fields are stripped.
#[test]
fn audit_parallel_output_is_deterministic() {
    let dir = create_audit_fixture("determinism");

    fs::write(
        dir.path().join("src/new.ts"),
        "export const dupA = (x: number) => x + 1;\nexport const dupB = (x: number) => x + 1;\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", "add new file"])
        .current_dir(dir.path())
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .unwrap();

    fn normalize(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("elapsed_ms");
                map.remove("head_sha");
                for v in map.values_mut() {
                    normalize(v);
                }
            }
            serde_json::Value::Array(items) => {
                for v in items {
                    normalize(v);
                }
            }
            _ => {}
        }
    }

    let mut canonicalized: Vec<String> = std::iter::repeat_with(|| {
        let output = run_fallow_raw(&[
            "audit",
            "--root",
            dir.path().to_str().unwrap(),
            "--base",
            "HEAD~1",
            "--format",
            "json",
            "--quiet",
        ]);
        assert!(
            output.code == 0 || output.code == 1,
            "audit run should not crash: stdout={}\nstderr={}",
            output.stdout,
            output.stderr
        );
        let mut value = parse_json(&output);
        normalize(&mut value);
        serde_json::to_string(&value).expect("re-serialize canonical json")
    })
    .take(3)
    .collect();

    let first = canonicalized.remove(0);
    for (idx, run) in canonicalized.iter().enumerate() {
        assert_eq!(
            &first,
            run,
            "audit parallel run #{} differed from run #0",
            idx + 1
        );
    }
}

#[test]
fn audit_json_has_summary_with_changes() {
    let dir = create_audit_fixture("summary");

    fs::write(
        dir.path().join("src/new.ts"),
        "export const newThing = 'added';\n",
    )
    .unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", "add new file"])
        .current_dir(dir.path())
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .unwrap();

    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.path().to_str().unwrap(),
        "--base",
        "HEAD~1",
        "--format",
        "json",
        "--quiet",
    ]);

    assert!(
        output.code == 0 || output.code == 1,
        "audit should not crash, got exit {}. stderr: {}",
        output.code,
        output.stderr
    );

    let json = parse_json(&output);
    assert!(
        json.get("summary").is_some(),
        "audit JSON should have summary"
    );
    let summary = &json["summary"];
    assert!(
        summary.get("dead_code_issues").is_some(),
        "summary should have dead_code_issues"
    );
}

// ---------------------------------------------------------------------------
// Audit baseline support (issue #139)
// ---------------------------------------------------------------------------

/// Create a fixture whose legacy file already has several unused exports,
/// then branch and touch that file without introducing new issues.
///
/// Returns the `TempDir` guard. The fixture is on a branch named
/// `feature`; the default branch is `main`.
fn create_audit_baseline_fixture() -> TempDir {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let dir = tmp.path();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r#"{"name": "audit-baseline-test", "main": "src/index.ts"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"ES2022","module":"ESNext","moduleResolution":"bundler"},"include":["src"]}"#,
    )
    .unwrap();

    // Legacy file with multiple pre-existing unused exports.
    fs::write(
        dir.join("src/legacy.ts"),
        "export const used = 1;\n\
         export const unusedA = 'a';\n\
         export const unusedB = 'b';\n\
         export const unusedC = 'c';\n\
         export const unusedD = 'd';\n\
         export const unusedE = 'e';\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { used } from './legacy';\nconsole.log(used);\n",
    )
    .unwrap();

    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git command failed")
    };

    git(&["init", "-b", "main"]);
    git(&["add", "."]);
    git(&["-c", "commit.gpgsign=false", "commit", "-m", "initial"]);
    git(&["checkout", "-b", "feature"]);

    // Touch the legacy file without adding new issues.
    let legacy = fs::read_to_string(dir.join("src/legacy.ts")).unwrap();
    fs::write(dir.join("src/legacy.ts"), format!("{legacy}// touched\n")).unwrap();
    git(&["add", "."]);
    git(&["-c", "commit.gpgsign=false", "commit", "-m", "touch legacy"]);

    tmp
}

#[test]
fn audit_default_gate_ignores_inherited_issues() {
    let tmp = create_audit_baseline_fixture();
    let output = run_fallow_raw(&[
        "audit",
        "--root",
        tmp.path().to_str().unwrap(),
        "--base",
        "main",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 0,
        "audit should pass when touched file has only inherited issues. stderr: {}",
        output.stderr
    );
    let json = parse_json(&output);
    assert_eq!(json["verdict"].as_str(), Some("pass"));
    let dead_code_issues = json["summary"]["dead_code_issues"]
        .as_u64()
        .expect("summary.dead_code_issues should be present");
    assert!(
        dead_code_issues >= 5,
        "expected at least 5 pre-existing unused exports, got {dead_code_issues}"
    );
    assert_eq!(
        json["attribution"]["dead_code_introduced"].as_u64(),
        Some(0)
    );
    assert!(
        json["attribution"]["dead_code_inherited"]
            .as_u64()
            .is_some_and(|count| count >= 5),
        "expected inherited dead-code attribution"
    );
    let inherited_exports = json["dead_code"]["unused_exports"]
        .as_array()
        .expect("dead_code.unused_exports should be an array");
    assert!(
        inherited_exports
            .iter()
            .all(|item| item["introduced"] == false),
        "all touched legacy exports should be annotated as inherited"
    );
}

#[test]
fn audit_gate_all_reports_preexisting_issues() {
    let tmp = create_audit_baseline_fixture();
    fs::write(tmp.path().join("fallow.toml"), "[audit]\ngate = \"all\"\n").unwrap();
    let output = run_fallow_raw(&[
        "audit",
        "--root",
        tmp.path().to_str().unwrap(),
        "--base",
        "main",
        "--config",
        tmp.path().join("fallow.toml").to_str().unwrap(),
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 1,
        "audit should fail when audit.gate=all and touched file has pre-existing issues. stderr: {}",
        output.stderr
    );
    let json = parse_json(&output);
    assert_eq!(json["verdict"].as_str(), Some("fail"));
    assert_eq!(json["attribution"]["gate"].as_str(), Some("all"));
    assert_eq!(
        json["attribution"]["dead_code_introduced"].as_u64(),
        Some(0),
        "gate=all should skip base attribution work"
    );
    assert_eq!(
        json["attribution"]["dead_code_inherited"].as_u64(),
        Some(0),
        "gate=all should skip base attribution work"
    );
    assert!(
        json["dead_code"]["unused_exports"][0]
            .get("introduced")
            .is_none(),
        "gate=all should not annotate per-issue introduced fields without a base snapshot"
    );
}

#[test]
fn audit_gate_cli_flag_overrides_default() {
    let tmp = create_audit_baseline_fixture();
    let output = run_fallow_raw(&[
        "audit",
        "--root",
        tmp.path().to_str().unwrap(),
        "--base",
        "main",
        "--gate",
        "all",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 1,
        "--gate all should fail on inherited findings. stderr: {}",
        output.stderr
    );
    let json = parse_json(&output);
    assert_eq!(json["verdict"].as_str(), Some("fail"));
    assert_eq!(json["attribution"]["gate"].as_str(), Some("all"));
    assert_eq!(
        json["attribution"]["dead_code_introduced"].as_u64(),
        Some(0)
    );
    assert_eq!(json["attribution"]["dead_code_inherited"].as_u64(), Some(0));
}

#[test]
fn audit_help_documents_gate() {
    let output = run_fallow_raw(&["audit", "--help"]);
    assert_eq!(output.code, 0, "audit --help should succeed");
    assert!(
        output.stdout.contains("--gate <GATE>"),
        "--help should include --gate, got:\n{}",
        output.stdout
    );
    assert!(
        output.stdout.contains("new-only") && output.stdout.contains("introduced"),
        "--help should document new-only semantics, got:\n{}",
        output.stdout
    );
}

#[test]
fn audit_new_unlisted_dependency_import_site_is_introduced() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let dir = tmp.path();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r#"{"name":"audit-unlisted","main":"src/index.ts","dependencies":{}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"ES2022","module":"ESNext","moduleResolution":"bundler"},"include":["src"]}"#,
    )
    .unwrap();
    fs::write(
        dir.join("src/a.ts"),
        "import leftPad from 'left-pad';\nexport const a = leftPad('a', 2);\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { a } from './a';\nconsole.log(a);\n",
    )
    .unwrap();
    git(dir, &["init", "-b", "main"]);
    commit_all(dir, "initial");

    fs::write(
        dir.join("src/b.ts"),
        "import leftPad from 'left-pad';\nexport const b = leftPad('b', 2);\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { a } from './a';\nimport { b } from './b';\nconsole.log(a, b);\n",
    )
    .unwrap();
    commit_all(dir, "add b");

    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.to_str().unwrap(),
        "--base",
        "HEAD~1",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 1,
        "new unlisted import site should fail new-only audit. stdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );
    let json = parse_json(&output);
    assert_eq!(json["verdict"].as_str(), Some("fail"));
    assert_eq!(
        json["attribution"]["dead_code_introduced"].as_u64(),
        Some(1)
    );
    assert_eq!(
        json["dead_code"]["unlisted_dependencies"][0]["introduced"],
        true
    );
}

#[test]
fn audit_dependency_location_change_is_introduced() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let dir = tmp.path();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r#"{"name":"audit-dep-move","main":"src/index.ts","devDependencies":{"left-pad":"1.0.0"}}"#,
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "console.log('hi');\n").unwrap();
    git(dir, &["init", "-b", "main"]);
    commit_all(dir, "initial");

    fs::write(
        dir.join("package.json"),
        r#"{"name":"audit-dep-move","main":"src/index.ts","dependencies":{"left-pad":"1.0.0"}}"#,
    )
    .unwrap();
    commit_all(dir, "move dependency");

    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.to_str().unwrap(),
        "--base",
        "HEAD~1",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 1,
        "moving an unused package into dependencies should be introduced. stdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );
    let json = parse_json(&output);
    assert_eq!(json["verdict"].as_str(), Some("fail"));
    assert_eq!(
        json["attribution"]["dead_code_introduced"].as_u64(),
        Some(1)
    );
    assert_eq!(
        json["dead_code"]["unused_dependencies"][0]["introduced"],
        true
    );
}

#[test]
fn audit_with_dead_code_baseline_filters_preexisting_issues() {
    let tmp = create_audit_baseline_fixture();
    let dir = tmp.path();
    let baseline_path = dir.join(".fallow-dead-code-baseline.json");

    // Save baseline from `main` state (before touching the file).
    // Switch back to main, save, then back to feature.
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git command failed")
    };
    git(&["checkout", "main"]);
    let save = run_fallow_raw(&[
        "dead-code",
        "--root",
        dir.to_str().unwrap(),
        "--save-baseline",
        baseline_path.to_str().unwrap(),
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(
        save.code == 0 || save.code == 1,
        "save-baseline should not crash, got {}: {}",
        save.code,
        save.stderr
    );
    assert!(
        baseline_path.exists(),
        "baseline file should have been written"
    );
    git(&["checkout", "feature"]);

    // Now audit with the dead-code baseline.
    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.to_str().unwrap(),
        "--base",
        "main",
        "--dead-code-baseline",
        baseline_path.to_str().unwrap(),
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 0,
        "audit with dead-code baseline should pass (no new issues). stdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );
    let json = parse_json(&output);
    assert_eq!(
        json["verdict"].as_str(),
        Some("pass"),
        "verdict should be pass when all pre-existing issues are baselined"
    );
    assert_eq!(
        json["summary"]["dead_code_issues"].as_u64(),
        Some(0),
        "baseline should filter all pre-existing unused exports"
    );
}

#[test]
fn audit_rejects_global_baseline_flag() {
    let tmp = create_audit_baseline_fixture();
    let output = run_fallow_raw(&[
        "--baseline",
        "anything.json",
        "audit",
        "--root",
        tmp.path().to_str().unwrap(),
        "--base",
        "main",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 2,
        "global --baseline on audit should exit 2. stderr: {}",
        output.stderr
    );
    let combined = format!("{}{}", output.stdout, output.stderr);
    assert!(
        combined.contains("--dead-code-baseline")
            || combined.contains("--health-baseline")
            || combined.contains("--dupes-baseline"),
        "error should point users at per-analysis flags, got: {combined}"
    );
}

#[test]
fn audit_rejects_global_save_baseline_flag() {
    let tmp = create_audit_baseline_fixture();
    let output = run_fallow_raw(&[
        "--save-baseline",
        "anywhere.json",
        "audit",
        "--root",
        tmp.path().to_str().unwrap(),
        "--base",
        "main",
        "--format",
        "json",
        "--quiet",
    ]);

    assert_eq!(
        output.code, 2,
        "global --save-baseline on audit should exit 2. stderr: {}",
        output.stderr
    );
    let combined = format!("{}{}", output.stdout, output.stderr);
    assert!(
        combined.contains("--dead-code-baseline")
            || combined.contains("--health-baseline")
            || combined.contains("--dupes-baseline"),
        "error should point users at per-analysis flags, got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// Audit error handling
// ---------------------------------------------------------------------------

#[test]
fn audit_badge_format_exits_2() {
    let dir = create_audit_fixture("badge");
    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.path().to_str().unwrap(),
        "--base",
        "HEAD",
        "--format",
        "badge",
        "--quiet",
    ]);
    assert_eq!(
        output.code, 2,
        "audit with --format badge should exit 2 (unsupported)"
    );
}

/// `--max-crap` on audit must flow into the health sub-analysis so that a
/// changed file with a high-complexity untested function triggers the
/// failing verdict.
#[test]
fn audit_max_crap_flag_fails_when_threshold_crossed() {
    let dir = create_audit_fixture("crap");

    // Introduce a file with a branchy, untested function. Combined with the
    // low `--max-crap 1`, any non-trivial cyclomatic count is guaranteed to
    // exceed the threshold.
    fs::write(
        dir.path().join("src/branchy.ts"),
        "export function branchy(n: number): number {\n\
           if (n < 0) return -1;\n\
           if (n === 0) return 0;\n\
           if (n < 10) return 1;\n\
           if (n < 100) return 2;\n\
           if (n < 1000) return 3;\n\
           if (n < 10000) return 4;\n\
           return 5;\n\
         }\n\
         import { used } from './legacy';\nbranchy(used);\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", "add branchy"])
        .current_dir(dir.path())
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .unwrap();

    let output = run_fallow_raw(&[
        "audit",
        "--root",
        dir.path().to_str().unwrap(),
        "--base",
        "HEAD~1",
        "--max-crap",
        "1",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(
        output.code, 1,
        "audit should fail when --max-crap is crossed. stderr: {}",
        output.stderr
    );
    let json = parse_json(&output);
    assert_eq!(
        json["verdict"].as_str(),
        Some("fail"),
        "verdict should be fail when CRAP threshold is crossed"
    );
}
