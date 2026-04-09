use super::common::{create_config, fixture_path};
use super::framework_convention_coverage_common::{
    collect_unused_exports, collect_unused_files, has_unused_export,
};

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
