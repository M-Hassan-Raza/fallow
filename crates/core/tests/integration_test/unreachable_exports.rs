use super::common::{create_config, fixture_path};

#[test]
fn unreachable_mixed_exports_flags_unused_export() {
    let root = fixture_path("unreachable-mixed-exports");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    // unusedHelper is exported but never imported by anyone — should be flagged
    assert!(
        unused_export_names.contains(&"unusedHelper"),
        "unusedHelper should be detected as unused export, found: {unused_export_names:?}"
    );
}

#[test]
fn unreachable_mixed_exports_flags_export_only_used_by_unreachable() {
    let root = fixture_path("unreachable-mixed-exports");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    // usedHelper is imported by setup.ts, but setup.ts is also unreachable,
    // so the reference shouldn't count — usedHelper should be flagged
    assert!(
        unused_export_names.contains(&"usedHelper"),
        "usedHelper (only referenced by unreachable module) should be flagged, found: {unused_export_names:?}"
    );
}
