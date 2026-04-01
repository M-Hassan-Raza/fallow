use super::common::{create_config, fixture_path};

// ---------------------------------------------------------------------------
// Hidden directory allowlist
// ---------------------------------------------------------------------------

#[test]
fn hidden_dir_allowlist_includes_storybook() {
    let root = fixture_path("hidden-dir-allowlist");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // .storybook/ is on the allowlist, so .storybook/main.ts should be discovered
    // and NOT reported as an unused file (it's imported by src/index.ts)
    let unused_files: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    assert!(
        !unused_files.iter().any(|f| f.contains(".storybook")),
        ".storybook/main.ts should not be in unused_files since it's imported. Got: {unused_files:?}"
    );
}

#[test]
fn hidden_dir_non_allowlisted_is_skipped() {
    let root = fixture_path("hidden-dir-allowlist");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // .hidden-other/ is NOT on the allowlist, so secret.ts should not be discovered at all
    let all_paths: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .chain(
            results
                .unused_exports
                .iter()
                .map(|e| e.path.to_string_lossy().to_string()),
        )
        .collect();

    assert!(
        !all_paths.iter().any(|f| f.contains(".hidden-other")),
        ".hidden-other/ should be completely skipped. Got: {all_paths:?}"
    );
}

// ---------------------------------------------------------------------------
// Astro file parsing
// ---------------------------------------------------------------------------

#[test]
fn astro_files_parsed_and_analyzed() {
    let root = fixture_path("astro-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // Orphan.astro should be in unused files since nothing imports it
    let unused_files: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    assert!(
        unused_files.iter().any(|f| f.contains("Orphan")),
        "Orphan.astro should be in unused_files. Got: {unused_files:?}"
    );
}

// ---------------------------------------------------------------------------
// MDX project
// ---------------------------------------------------------------------------

#[test]
fn mdx_unused_file_detected() {
    let root = fixture_path("mdx-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_files: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    assert!(
        unused_files.iter().any(|f| f.contains("Unused")),
        "Unused.tsx should be in unused_files. Got: {unused_files:?}"
    );
}

// ---------------------------------------------------------------------------
// Complexity project (used by health tests)
// ---------------------------------------------------------------------------

#[test]
fn complexity_project_analyzes_without_errors() {
    let root = fixture_path("complexity-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    assert!(
        results.unresolved_imports.is_empty(),
        "complexity-project should have no unresolved imports"
    );
}

// ---------------------------------------------------------------------------
// Error handling: no package.json
// ---------------------------------------------------------------------------

#[test]
fn error_no_package_json_produces_empty_results() {
    let root = fixture_path("error-no-package-json");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    assert_eq!(
        results.total_issues(),
        0,
        "no package.json should produce empty results"
    );
}
