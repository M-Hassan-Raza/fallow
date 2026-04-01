#[path = "common/mod.rs"]
mod common;

use common::{fixture_path, parse_json, redact_all, run_fallow};

// ---------------------------------------------------------------------------
// JSON output structure
// ---------------------------------------------------------------------------

#[test]
fn health_json_output_is_valid() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--format", "json", "--quiet"],
    );
    assert_eq!(output.code, 0, "health should succeed");
    let json = parse_json(&output);
    assert!(json.is_object(), "health JSON output should be an object");
}

#[test]
fn health_json_has_findings() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--complexity", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    // complexity-project has a function with cyclomatic > 10
    assert!(
        json.get("findings").is_some(),
        "health JSON should have findings key"
    );
}

// ---------------------------------------------------------------------------
// Exit code with threshold
// ---------------------------------------------------------------------------

#[test]
fn health_exits_0_below_threshold() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &[
            "--max-cyclomatic",
            "50",
            "--complexity",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 0,
        "health should exit 0 when complexity below threshold"
    );
}

#[test]
fn health_exits_1_when_threshold_exceeded() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &[
            "--max-cyclomatic",
            "3",
            "--complexity",
            "--fail-on-issues",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 1,
        "health should exit 1 when complexity exceeds threshold"
    );
}

// ---------------------------------------------------------------------------
// Section flags
// ---------------------------------------------------------------------------

#[test]
fn health_score_flag_shows_score() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--score", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    assert!(
        json.get("score").is_some() || json.get("health_score").is_some(),
        "health --score should include score data"
    );
}

#[test]
fn health_file_scores_flag() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--file-scores", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    assert!(
        json.get("file_scores").is_some(),
        "health --file-scores should include file_scores"
    );
}

// ---------------------------------------------------------------------------
// Human output snapshot
// ---------------------------------------------------------------------------

#[test]
fn health_human_output_snapshot() {
    // Use --max-cyclomatic 10 so the 14-branch classify() function exceeds the threshold
    // and produces actual output to snapshot (default threshold of 20 would show nothing)
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--complexity", "--max-cyclomatic", "10", "--quiet"],
    );
    let root = fixture_path("complexity-project");
    let redacted = redact_all(&output.stdout, &root);
    insta::assert_snapshot!("health_human_complexity", redacted);
}
