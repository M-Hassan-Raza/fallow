use crate::common::{git, parse_json, run_fallow_raw};
use std::fs;
use std::path::Path;

fn write_config(root: &Path, web_ref: &str) {
    fs::write(
        root.join(".fallowrc.json"),
        format!(
            r#"{{"workspaces":{{"changedSince":{{"packages/web":"{web_ref}","packages/legacy":"HEAD"}}}}}}"#
        ),
    )
    .expect("config");
}

fn unused_export_paths(root: &Path, args: &[&str]) -> Vec<String> {
    let mut command = vec!["check", "--root", root.to_str().expect("root path")];
    command.extend_from_slice(args);
    command.extend_from_slice(&["--format", "json", "--quiet"]);
    let output = run_fallow_raw(&command);
    assert!(
        output.code == 0 || output.code == 1,
        "check failed: {}",
        output.stderr
    );
    parse_json(&output)["unused_exports"]
        .as_array()
        .expect("unused exports")
        .iter()
        .filter_map(|finding| finding["path"].as_str())
        .map(|path| path.replace('\\', "/"))
        .collect()
}

#[test]
fn package_baselines_scope_each_workspace_and_global_ref_overrides() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::write(
        root.join("package.json"),
        r#"{"name":"scope-root","private":true,"workspaces":["packages/*"]}"#,
    )
    .expect("root manifest");
    for name in ["web", "legacy"] {
        let package = root.join("packages").join(name);
        fs::create_dir_all(package.join("src")).expect("package source directory");
        fs::write(
            package.join("package.json"),
            format!(r#"{{"name":"{name}","main":"src/index.ts"}}"#),
        )
        .expect("package manifest");
        fs::write(
            package.join("src/index.ts"),
            "import { used } from './utils';\nused();\n",
        )
        .expect("entry");
        fs::write(
            package.join("src/utils.ts"),
            format!("export const used = () => 1;\nexport const unused_{name} = 1;\n"),
        )
        .expect("utilities");
    }
    write_config(root, "HEAD~1");
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Fallow Test",
            "-c",
            "user.email=fallow@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "base",
        ],
    );
    fs::write(
        root.join("packages/web/src/utils.ts"),
        "export const used = () => 2;\nexport const unused_web = 2;\n",
    )
    .expect("web change");
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Fallow Test",
            "-c",
            "user.email=fallow@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "web change",
        ],
    );
    fs::write(
        root.join("packages/legacy/src/utils.ts"),
        "export const used = () => 3;\nexport const unused_legacy = 3;\n",
    )
    .expect("legacy change");

    let paths = unused_export_paths(root, &[]);
    let report = run_fallow_raw(&[
        "check",
        "--root",
        root.to_str().unwrap(),
        "--format",
        "json",
        "--quiet",
    ]);
    let json = parse_json(&report);
    assert_eq!(
        json["package_baselines"],
        serde_json::json!([
            {"workspace_root":"packages/legacy","reference":"HEAD"},
            {"workspace_root":"packages/web","reference":"HEAD~1"}
        ])
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("packages/web/src/utils.ts"))
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("packages/legacy/src/utils.ts"))
    );

    write_config(root, "missing-ref");
    let global_paths = unused_export_paths(root, &["--changed-since", "HEAD"]);
    let global_report = run_fallow_raw(&[
        "check",
        "--root",
        root.to_str().unwrap(),
        "--changed-since",
        "HEAD",
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(
        parse_json(&global_report)
            .get("package_baselines")
            .is_none()
    );
    assert!(
        global_paths
            .iter()
            .any(|path| path.ends_with("packages/legacy/src/utils.ts"))
    );
    assert!(
        !global_paths
            .iter()
            .any(|path| path.ends_with("packages/web/src/utils.ts"))
    );

    let failed = run_fallow_raw(&[
        "check",
        "--root",
        root.to_str().expect("root path"),
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(failed.code, 2, "{}", failed.stderr);
    assert!(
        failed.stderr.contains("Workspace baseline error")
            || failed.stdout.contains("Workspace baseline error"),
        "stdout: {}; stderr: {}",
        failed.stdout,
        failed.stderr
    );
}
