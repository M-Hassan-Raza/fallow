//! Scope flags of `fallow_api`, the runtime behind the MCP typed route and the
//! Node bindings. A scope must narrow the same way as on the CLI.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests use unwrap and expect to keep fixture setup concise"
)]

use std::path::Path;

use fallow_api::{
    AnalysisOptions, CombinedOptions, DeadCodeOptions, DuplicationOptions, run_combined,
    run_dead_code, run_duplication, serialize_combined_programmatic_json,
    serialize_dead_code_programmatic_json, serialize_duplication_programmatic_json,
};
use serde_json::Value;

use crate::common::{commit, git, write};

fn analysis(root: &Path) -> AnalysisOptions {
    AnalysisOptions {
        root: Some(root.to_path_buf()),
        no_cache: true,
        ..AnalysisOptions::default()
    }
}

fn dead_code(analysis: AnalysisOptions) -> Value {
    let options = DeadCodeOptions {
        analysis,
        ..DeadCodeOptions::default()
    };
    run_dead_code(&options)
        .and_then(serialize_dead_code_programmatic_json)
        .expect("run the programmatic dead-code analysis")
}

#[test]
fn package_git_baselines_apply_to_programmatic_dead_code() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(
        root,
        "package.json",
        r#"{"name":"root","private":true,"workspaces":["packages/*"]}"#,
    );
    write(
        root,
        ".fallowrc.json",
        r#"{"workspaces":{"changedSince":{"packages/web":"HEAD~1","packages/legacy":"HEAD"}}}"#,
    );
    for name in ["web", "legacy"] {
        write(
            root,
            &format!("packages/{name}/package.json"),
            &format!(r#"{{"name":"{name}","main":"src/index.ts"}}"#),
        );
        write(
            root,
            &format!("packages/{name}/src/index.ts"),
            "import { used } from './utils';\nused();\n",
        );
        write(
            root,
            &format!("packages/{name}/src/utils.ts"),
            &format!("export const used = () => 1;\nexport const unused_{name} = 1;\n"),
        );
    }
    git(root, &["init", "-q"]);
    commit(root, "base");
    write(
        root,
        "packages/web/src/utils.ts",
        "export const used = () => 2;\nexport const unused_web = 2;\n",
    );
    commit(root, "web change");
    write(
        root,
        "packages/legacy/src/utils.ts",
        "export const used = () => 3;\nexport const unused_legacy = 3;\n",
    );

    let paths = |report: &Value| {
        report["unused_exports"]
            .as_array()
            .expect("unused exports")
            .iter()
            .filter_map(|finding| finding["path"].as_str())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    };
    let package_report = dead_code(analysis(root));
    assert_eq!(
        package_report["package_baselines"],
        serde_json::json!([
            {"workspace_root":"packages/legacy","reference":"HEAD"},
            {"workspace_root":"packages/web","reference":"HEAD~1"}
        ])
    );
    let package_paths = paths(&package_report);
    assert!(
        package_paths
            .iter()
            .any(|path| path.contains("packages/web/"))
    );
    assert!(
        package_paths
            .iter()
            .any(|path| path.contains("packages/legacy/"))
    );

    let global_report = dead_code(AnalysisOptions {
        changed_since: Some("HEAD".to_owned()),
        ..analysis(root)
    });
    assert!(global_report.get("package_baselines").is_none());
    let global_paths = paths(&global_report);
    assert!(
        !global_paths
            .iter()
            .any(|path| path.contains("packages/web/"))
    );
    assert!(
        global_paths
            .iter()
            .any(|path| path.contains("packages/legacy/"))
    );
}

/// The locations of each `duplicate_exports` finding, as relative paths.
fn duplicate_export_owners(report: &Value) -> Vec<Vec<String>> {
    report["duplicate_exports"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|finding| {
            finding["locations"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|location| location["path"].as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

/// `dup` is exported by three files. `ignoreFindings` matches two of them, so
/// the full run reports the finding: not every owner is ignored.
fn duplicate_export_repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(
        root,
        "package.json",
        r#"{"name":"scope-parity","version":"1.0.0","main":"src/index.ts"}"#,
    );
    write(
        root,
        ".fallowrc.json",
        r#"{"ignoreFindings":["src/x.ts","src/y.ts"]}"#,
    );
    write(
        root,
        "src/index.ts",
        "export * from \"./x\";\nexport * from \"./y\";\nexport * from \"./z\";\n",
    );
    for name in ["x", "y", "z"] {
        write(root, &format!("src/{name}.ts"), "export const dup = 1;\n");
    }
    git(root, &["init", "-q"]);
    commit(root, "base");
    write(root, "src/x.ts", "export const dup = 2;\n");
    write(root, "src/y.ts", "export const dup = 3;\n");
    commit(root, "head");
    dir
}

#[test]
fn a_full_run_reports_a_duplicate_export_with_one_owner_not_ignored() {
    let dir = duplicate_export_repository();
    let report = dead_code(analysis(dir.path()));
    assert_eq!(
        duplicate_export_owners(&report),
        vec![vec!["src/x.ts", "src/y.ts", "src/z.ts"]]
    );
}

#[test]
fn a_duplicate_export_that_only_ignored_owners_hold_after_the_scope_is_hidden() {
    let dir = duplicate_export_repository();
    let report = dead_code(AnalysisOptions {
        changed_since: Some("HEAD~1".to_string()),
        ..analysis(dir.path())
    });
    assert_eq!(
        duplicate_export_owners(&report),
        Vec::<Vec<String>>::new(),
        "`--changed-since` keeps the owners src/x.ts and src/y.ts, which \
         `ignoreFindings` both match, so the finding is hidden as on the CLI"
    );
}

/// The same 13-line function in `packages/a` (`pkg-a`) and `packages/b`
/// (`pkg-b`): one clone group with one instance in each workspace.
fn cross_workspace_clone_repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(
        root,
        "package.json",
        r#"{"name":"root","private":true,"workspaces":["packages/*"]}"#,
    );
    let function = "export function compute(input: number): number {\n  let total = input;\n  \
                    for (let index = 0; index < 10; index += 1) {\n    total += index * 2;\n    \
                    if (total > 100) {\n      total -= 7;\n    }\n  }\n  const scaled = total * 3;\n  \
                    const shifted = scaled - 11;\n  const clamped = Math.min(shifted, 999);\n  \
                    return clamped + input;\n}\n";
    for (dir_name, package) in [("a", "pkg-a"), ("b", "pkg-b")] {
        write(
            root,
            &format!("packages/{dir_name}/package.json"),
            &format!(r#"{{"name":"{package}","version":"1.0.0","main":"src/index.ts"}}"#),
        );
        write(
            root,
            &format!("packages/{dir_name}/src/index.ts"),
            "export { compute } from \"./clone\";\n",
        );
        write(root, &format!("packages/{dir_name}/src/clone.ts"), function);
    }
    git(root, &["init", "-q"]);
    commit(root, "base");
    dir
}

/// The instance paths of each clone group, relative to the root.
fn clone_group_files(report: &Value) -> Vec<Vec<String>> {
    report["clone_groups"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|group| {
            let mut files: Vec<String> = group["instances"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|instance| instance["file"].as_str().unwrap().to_string())
                .collect();
            files.sort();
            files.dedup();
            files
        })
        .collect()
}

#[test]
fn package_baselines_scope_standalone_and_combined_duplication() {
    let dir = cross_workspace_clone_repository();
    let root = dir.path();
    write(
        root,
        ".fallowrc.json",
        r#"{"workspaces":{"changedSince":{"packages/a":"HEAD","packages/b":"HEAD"}}}"#,
    );
    assert!(clone_group_files(&duplication(analysis(root))).is_empty());
    assert_eq!(
        duplication(analysis(root))["package_baselines"],
        serde_json::json!([
            {"workspace_root":"packages/a","reference":"HEAD"},
            {"workspace_root":"packages/b","reference":"HEAD"}
        ])
    );

    let combined = serialize_combined_programmatic_json(
        run_combined(&CombinedOptions {
            analysis: analysis(root),
            dead_code: false,
            health: false,
            ..CombinedOptions::default()
        })
        .expect("combined analysis"),
    )
    .expect("combined JSON");
    assert!(
        combined["dupes"]["clone_groups"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert_eq!(
        combined["package_baselines"],
        duplication(analysis(root))["package_baselines"]
    );

    write(
        root,
        ".fallowrc.json",
        r#"{"workspaces":{"changedSince":{"packages/a":"HEAD"}}}"#,
    );
    assert!(
        !clone_group_files(&duplication(analysis(root))).is_empty(),
        "a clone group touching the unmapped full-scope package must survive"
    );

    write(
        root,
        ".fallowrc.json",
        r#"{"workspaces":{"changedSince":{"packages/a":"missing-ref"}}}"#,
    );
    let err = run_duplication(&DuplicationOptions {
        analysis: analysis(root),
        ..DuplicationOptions::default()
    })
    .expect_err("invalid configured refs fail duplication");
    assert_eq!(err.code.as_deref(), Some("FALLOW_PACKAGE_BASELINE_FAILED"));
}

fn duplication(analysis: AnalysisOptions) -> Value {
    let options = DuplicationOptions {
        analysis,
        ..DuplicationOptions::default()
    };
    run_duplication(&options)
        .and_then(serialize_duplication_programmatic_json)
        .expect("run the programmatic duplication analysis")
}

#[test]
fn a_workspace_scope_keeps_a_clone_group_with_one_instance_in_the_workspace() {
    let dir = cross_workspace_clone_repository();
    let unscoped = clone_group_files(&duplication(analysis(dir.path())));
    let cross = vec![
        "packages/a/src/clone.ts".to_string(),
        "packages/b/src/clone.ts".to_string(),
    ];
    assert!(
        unscoped.contains(&cross),
        "the fixture must hold a clone group across the two workspaces: {unscoped:?}"
    );

    let scoped = clone_group_files(&duplication(AnalysisOptions {
        workspace: Some(vec!["pkg-a".to_string()]),
        ..analysis(dir.path())
    }));
    assert!(
        scoped.contains(&cross),
        "`workspace: pkg-a` keeps the whole group, as `fallow dupes --workspace pkg-a` \
         does: a clone group is in scope when one of its instances is. Got {scoped:?}"
    );
}
