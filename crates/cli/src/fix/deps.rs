use rustc_hash::FxHashMap;
use std::path::Path;

use fallow_config::OutputFormat;

use super::io::atomic_write;

/// Apply dependency fixes to package.json files (root and workspace), returning JSON fix entries.
pub(super) fn apply_dependency_fixes(
    root: &Path,
    results: &fallow_core::results::AnalysisResults,
    output: &OutputFormat,
    dry_run: bool,
    fixes: &mut Vec<serde_json::Value>,
) -> bool {
    let mut had_write_error = false;

    if results.unused_dependencies.is_empty()
        && results.unused_dev_dependencies.is_empty()
        && results.unused_optional_dependencies.is_empty()
    {
        return had_write_error;
    }

    // Group all unused deps by their package.json path so we can batch edits per file
    let mut deps_by_pkg: FxHashMap<&Path, Vec<(&str, &str)>> = FxHashMap::default();
    for dep in &results.unused_dependencies {
        deps_by_pkg
            .entry(&dep.path)
            .or_default()
            .push((&dep.package_name, "dependencies"));
    }
    for dep in &results.unused_dev_dependencies {
        deps_by_pkg
            .entry(&dep.path)
            .or_default()
            .push((&dep.package_name, "devDependencies"));
    }
    for dep in &results.unused_optional_dependencies {
        deps_by_pkg
            .entry(&dep.path)
            .or_default()
            .push((&dep.package_name, "optionalDependencies"));
    }

    let _ = root; // root was previously used to construct the path; now deps carry their own path

    for (pkg_path, removals) in &deps_by_pkg {
        if let Ok(content) = std::fs::read_to_string(pkg_path)
            && let Ok(mut pkg_value) = serde_json::from_str::<serde_json::Value>(&content)
        {
            let mut changed = false;

            for &(package_name, location) in removals {
                if let Some(deps) = pkg_value.get_mut(location)
                    && let Some(obj) = deps.as_object_mut()
                    && obj.remove(package_name).is_some()
                {
                    if dry_run {
                        if !matches!(output, OutputFormat::Json) {
                            eprintln!(
                                "Would remove `{package_name}` from {location} in {}",
                                pkg_path.display()
                            );
                        }
                        fixes.push(serde_json::json!({
                            "type": "remove_dependency",
                            "package": package_name,
                            "location": location,
                            "file": pkg_path.display().to_string(),
                        }));
                    } else {
                        changed = true;
                        fixes.push(serde_json::json!({
                            "type": "remove_dependency",
                            "package": package_name,
                            "location": location,
                            "file": pkg_path.display().to_string(),
                            "applied": true,
                        }));
                    }
                }
            }

            if changed && !dry_run {
                match serde_json::to_string_pretty(&pkg_value) {
                    Ok(new_json) => {
                        let pkg_content = new_json + "\n";
                        if let Err(e) = atomic_write(pkg_path, pkg_content.as_bytes()) {
                            had_write_error = true;
                            eprintln!("Error: failed to write {}: {e}", pkg_path.display());
                        }
                    }
                    Err(e) => {
                        had_write_error = true;
                        eprintln!("Error: failed to serialize {}: {e}", pkg_path.display());
                    }
                }
            }
        }
    }

    had_write_error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_fix_dry_run_does_not_modify_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pkg_path = root.join("package.json");
        let original =
            r#"{"dependencies": {"lodash": "^4.0.0"}, "devDependencies": {"jest": "^29.0.0"}}"#;
        std::fs::write(&pkg_path, original).unwrap();

        let mut results = fallow_core::results::AnalysisResults::default();
        results
            .unused_dependencies
            .push(fallow_core::results::UnusedDependency {
                package_name: "lodash".into(),
                location: fallow_core::results::DependencyLocation::Dependencies,
                path: pkg_path.clone(),
            });

        let mut fixes = Vec::new();
        apply_dependency_fixes(root, &results, &OutputFormat::Json, true, &mut fixes);

        // package.json should not change
        assert_eq!(std::fs::read_to_string(&pkg_path).unwrap(), original);
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0]["type"], "remove_dependency");
        assert_eq!(fixes[0]["package"], "lodash");
    }

    #[test]
    fn dependency_fix_removes_unused_dep_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pkg_path = root.join("package.json");
        std::fs::write(
            &pkg_path,
            r#"{"dependencies": {"lodash": "^4.0.0", "react": "^18.0.0"}}"#,
        )
        .unwrap();

        let mut results = fallow_core::results::AnalysisResults::default();
        results
            .unused_dependencies
            .push(fallow_core::results::UnusedDependency {
                package_name: "lodash".into(),
                location: fallow_core::results::DependencyLocation::Dependencies,
                path: pkg_path.clone(),
            });

        let mut fixes = Vec::new();
        let had_error =
            apply_dependency_fixes(root, &results, &OutputFormat::Human, false, &mut fixes);

        assert!(!had_error);
        let content = std::fs::read_to_string(&pkg_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let deps = parsed["dependencies"].as_object().unwrap();
        assert!(!deps.contains_key("lodash"));
        assert!(deps.contains_key("react"));
    }

    #[test]
    fn dependency_fix_empty_results_returns_early() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let results = fallow_core::results::AnalysisResults::default();
        let mut fixes = Vec::new();
        let had_error =
            apply_dependency_fixes(root, &results, &OutputFormat::Human, false, &mut fixes);
        assert!(!had_error);
        assert!(fixes.is_empty());
    }
}
