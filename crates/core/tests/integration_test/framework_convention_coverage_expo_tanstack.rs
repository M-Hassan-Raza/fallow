use std::{fs, path::Path};

use super::common::{create_config, fixture_path};
use super::framework_convention_coverage_common::{
    collect_unused_exports, collect_unused_files, has_unused_export,
};
use tempfile::tempdir;

fn write_project_file(root: &Path, relative_path: &str, source: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, source).expect("write test file");
}

#[test]
fn expo_router_special_files_and_exports_are_covered() {
    let root = fixture_path("expo-router-conventions");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_files = collect_unused_files(&root, &results);
    assert!(
        !unused_files.iter().any(|path| path == "src/app/index.tsx"),
        "configured route root should be treated as entry points, unused files: {unused_files:?}"
    );
    assert!(
        unused_files.iter().any(|path| path == "app/legacy.tsx"),
        "default app/ directory should not stay alive when expo-router root is src/app: {unused_files:?}"
    );

    let unused_exports = collect_unused_exports(&root, &results);
    for (path, export) in [
        ("src/app/_layout.tsx", "default"),
        ("src/app/_layout.tsx", "ErrorBoundary"),
        ("src/app/_layout.tsx", "unstable_settings"),
        ("src/app/index.tsx", "default"),
        ("src/app/index.tsx", "ErrorBoundary"),
        ("src/app/index.tsx", "loader"),
        ("src/app/index.tsx", "generateStaticParams"),
        ("src/app/+html.tsx", "default"),
        ("src/app/+not-found.tsx", "default"),
        ("src/app/+native-intent.tsx", "redirectSystemPath"),
        ("src/app/+native-intent.tsx", "legacy_subscribe"),
        ("src/app/+middleware.ts", "default"),
        ("src/app/+middleware.ts", "unstable_settings"),
        ("src/app/hello+api.ts", "GET"),
        ("src/app/hello+api.ts", "POST"),
    ] {
        assert!(
            !has_unused_export(&unused_exports, path, export),
            "{path}:{export} should be framework-used, found: {unused_exports:?}"
        );
    }

    for (path, export) in [
        ("src/app/_layout.tsx", "unusedLayoutHelper"),
        ("src/app/index.tsx", "unusedIndexHelper"),
        ("src/app/+html.tsx", "unusedHtmlHelper"),
        ("src/app/+not-found.tsx", "unusedNotFoundHelper"),
        ("src/app/+native-intent.tsx", "unusedIntentHelper"),
        ("src/app/+middleware.ts", "unusedMiddlewareHelper"),
        ("src/app/hello+api.ts", "unusedApiHelper"),
    ] {
        assert!(
            has_unused_export(&unused_exports, path, export),
            "{path}:{export} should still be reported as unused, found: {unused_exports:?}"
        );
    }
}

#[test]
fn tanstack_router_custom_route_dir_and_lazy_exports_are_covered() {
    let root = fixture_path("tanstack-router-conventions");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_files = collect_unused_files(&root, &results);
    assert!(
        !unused_files
            .iter()
            .any(|path| path == "app/pages/index.tsx"),
        "custom route dir should be reachable through generated route tree, unused files: {unused_files:?}"
    );
    assert!(
        !unused_files
            .iter()
            .any(|path| path == "src/routeTree.gen.ts"),
        "custom route dir should not relocate the default generated route tree path, unused files: {unused_files:?}"
    );
    assert!(
        !unused_files.iter().any(|path| path == "src/router.ts"),
        "custom route dir should not relocate the default router entry path, unused files: {unused_files:?}"
    );
    assert!(
        unused_files
            .iter()
            .any(|path| path == "src/routes/legacy.tsx"),
        "default src/routes should not stay alive when tsr.config.json points elsewhere: {unused_files:?}"
    );

    let unused_exports = collect_unused_exports(&root, &results);
    for (path, export) in [
        ("app/pages/__root.tsx", "Route"),
        ("app/pages/index.tsx", "Route"),
        ("app/pages/index.tsx", "loader"),
        ("app/pages/index.tsx", "beforeLoad"),
        ("app/pages/posts.lazy.tsx", "Route"),
        ("app/pages/posts.lazy.tsx", "component"),
        ("app/pages/posts.lazy.tsx", "pendingComponent"),
    ] {
        assert!(
            !has_unused_export(&unused_exports, path, export),
            "{path}:{export} should be framework-used, found: {unused_exports:?}"
        );
    }

    for (path, export) in [
        ("app/pages/__root.tsx", "unusedRootHelper"),
        ("app/pages/index.tsx", "unusedIndexHelper"),
        ("app/pages/posts.lazy.tsx", "unusedLazyHelper"),
    ] {
        assert!(
            has_unused_export(&unused_exports, path, export),
            "{path}:{export} should still be reported as unused, found: {unused_exports:?}"
        );
    }
}

#[test]
fn tanstack_router_prefix_and_ignore_patterns_stay_strict() {
    let root = fixture_path("tanstack-router-prefix-and-ignore");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_files = collect_unused_files(&root, &results);
    for path in ["src/routes/helper.tsx", "src/routes/ignored.page.tsx"] {
        assert!(
            unused_files.iter().any(|unused| unused == path),
            "{path} should not be treated as a live route file, unused files: {unused_files:?}"
        );
    }
    for path in [
        "src/routes/route-home.tsx",
        "src/routes/route-posts.lazy.tsx",
    ] {
        assert!(
            !unused_files.iter().any(|unused| unused == path),
            "{path} should stay reachable as a configured route file, unused files: {unused_files:?}"
        );
    }

    let unused_exports = collect_unused_exports(&root, &results);
    for (path, export) in [
        ("src/routes/route-home.tsx", "Route"),
        ("src/routes/route-posts.lazy.tsx", "Route"),
        ("src/routes/route-posts.lazy.tsx", "component"),
    ] {
        assert!(
            !has_unused_export(&unused_exports, path, export),
            "{path}:{export} should be framework-used, found: {unused_exports:?}"
        );
    }
    assert!(
        has_unused_export(&unused_exports, "src/routes/route-posts.lazy.tsx", "loader"),
        "lazy routes should not inherit non-lazy exports, found: {unused_exports:?}"
    );
}

#[test]
fn tanstack_router_custom_route_dir_replaces_default_used_export_rules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();

    write_project_file(
        root,
        "package.json",
        r#"{
  "dependencies": {
    "@tanstack/react-router": "1.0.0"
  }
}"#,
    );
    write_project_file(
        root,
        "tsr.config.json",
        r#"{
  "routesDirectory": "./app/pages"
}"#,
    );
    write_project_file(
        root,
        "app/pages/index.tsx",
        "import '../shared';\nexport const Route = {};\n",
    );
    write_project_file(
        root,
        "app/shared.ts",
        "import { helper } from '../src/routes/legacy';\nconsole.log(helper);\n",
    );
    write_project_file(
        root,
        "src/routes/legacy.tsx",
        "export const Route = {};\nexport const helper = 1;\n",
    );

    let config = create_config(root.to_path_buf());
    let results = fallow_core::analyze(&config).expect("analysis should succeed");
    let unused_files = collect_unused_files(root, &results);
    assert!(
        !unused_files.iter().any(|path| path == "src/routes/legacy.tsx"),
        "helper import should keep the legacy file reachable, unused files: {unused_files:?}"
    );

    let unused_exports = collect_unused_exports(root, &results);
    assert!(
        has_unused_export(&unused_exports, "src/routes/legacy.tsx", "Route"),
        "default route-dir exports should not stay framework-used after routesDirectory moves, found: {unused_exports:?}"
    );
    assert!(
        !has_unused_export(&unused_exports, "src/routes/legacy.tsx", "helper"),
        "regular live exports should stay used, found: {unused_exports:?}"
    );
}

#[test]
fn tanstack_router_invalid_ignore_pattern_only_drops_the_bad_filter() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();

    write_project_file(
        root,
        "package.json",
        r#"{
  "dependencies": {
    "@tanstack/react-router": "1.0.0"
  }
}"#,
    );
    write_project_file(
        root,
        "tsr.config.json",
        r#"{
  "routeFileIgnorePattern": "["
}"#,
    );
    write_project_file(root, "src/routes/index.tsx", "export const Route = {};\n");

    let config = create_config(root.to_path_buf());
    let results = fallow_core::analyze(&config).expect("analysis should succeed");
    let unused_files = collect_unused_files(root, &results);
    assert!(
        !unused_files.iter().any(|path| path == "src/routes/index.tsx"),
        "invalid ignore patterns should not disable route discovery, unused files: {unused_files:?}"
    );

    let unused_exports = collect_unused_exports(root, &results);
    assert!(
        !has_unused_export(&unused_exports, "src/routes/index.tsx", "Route"),
        "invalid ignore patterns should not disable framework-used export rules, found: {unused_exports:?}"
    );
}
