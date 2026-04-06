use super::common::{create_config, fixture_path};

// ── Framework entry points (Next.js) ───────────────────────────

#[test]
fn nextjs_page_default_export_not_flagged() {
    let root = fixture_path("nextjs-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // page.tsx is a Next.js App Router entry point, so it should NOT be unused
    assert!(
        !unused_file_names.contains(&"page.tsx".to_string()),
        "page.tsx should be treated as framework entry point, unused files: {unused_file_names:?}"
    );

    // utils.ts is not imported by anything, so it should be unused
    assert!(
        unused_file_names.contains(&"utils.ts".to_string()),
        "utils.ts should be detected as unused file, found: {unused_file_names:?}"
    );
}

#[test]
fn nextjs_unused_util_export_flagged() {
    let root = fixture_path("nextjs-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // unusedUtil is exported but never imported — however, since utils.ts is an
    // unreachable file, it may be reported as unused file instead of unused export.
    // The key point is that it IS flagged as a problem in some way.
    let has_unused_export = results
        .unused_exports
        .iter()
        .any(|e| e.export_name == "unusedUtil");
    let has_unused_file = results
        .unused_files
        .iter()
        .any(|f| f.path.file_name().is_some_and(|n| n == "utils.ts"));

    assert!(
        has_unused_export || has_unused_file,
        "unusedUtil should be flagged as unused export or utils.ts as unused file"
    );
}

#[test]
fn nextjs_convention_exports_are_not_flagged() {
    let root = fixture_path("nextjs-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    for expected_used in [
        "revalidate",
        "dynamic",
        "generateMetadata",
        "viewport",
        "GET",
        "runtime",
        "preferredRegion",
        "proxy",
        "config",
        "register",
        "onRequestError",
        "onRouterTransitionStart",
        "reportWebVitals",
    ] {
        assert!(
            !unused_export_names.contains(&expected_used),
            "{expected_used} should be treated as a framework-used Next.js export, found: {unused_export_names:?}"
        );
    }
}

#[test]
fn nextjs_special_file_exports_are_not_flagged() {
    let root = fixture_path("nextjs-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_exports: Vec<(String, String)> = results
        .unused_exports
        .iter()
        .map(|e| {
            (
                e.path.file_name().unwrap().to_string_lossy().to_string(),
                e.export_name.clone(),
            )
        })
        .collect();

    for (file, export) in [
        ("loading.tsx", "default"),
        ("error.tsx", "default"),
        ("not-found.tsx", "default"),
        ("template.tsx", "default"),
        ("default.tsx", "default"),
        ("global-error.tsx", "default"),
        ("global-not-found.tsx", "default"),
        ("global-not-found.tsx", "metadata"),
        ("mdx-components.tsx", "useMDXComponents"),
    ] {
        assert!(
            !unused_exports
                .iter()
                .any(|(unused_file, unused_export)| unused_file == file && unused_export == export),
            "{file}:{export} should be treated as framework-used, found: {unused_exports:?}"
        );
    }

    for (file, export) in [
        ("loading.tsx", "unusedLoadingHelper"),
        ("proxy.ts", "unusedProxyHelper"),
        ("instrumentation.ts", "unusedInstrumentationHelper"),
        ("instrumentation-client.ts", "unusedClientHelper"),
        ("mdx-components.tsx", "unusedMdxHelper"),
        ("global-not-found.tsx", "unusedGlobalNotFoundHelper"),
    ] {
        assert!(
            unused_exports
                .iter()
                .any(|(unused_file, unused_export)| unused_file == file && unused_export == export),
            "{file}:{export} should still be reported as unused, found: {unused_exports:?}"
        );
    }
}

#[test]
fn nextjs_config_referenced_dependencies_are_not_flagged_unused() {
    let root = fixture_path("nextjs-config-deps");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_dep_names: Vec<&str> = results
        .unused_dependencies
        .iter()
        .map(|d| d.package_name.as_str())
        .collect();

    assert!(
        !unused_dep_names.contains(&"@acme/ui"),
        "@acme/ui should be treated as used via next.config transpilePackages: {unused_dep_names:?}"
    );
    assert!(
        unused_dep_names.contains(&"left-pad"),
        "left-pad should remain unused as a control dependency: {unused_dep_names:?}"
    );
}

// ── Path aliases ───────────────────────────────────────────────

#[test]
fn path_alias_not_flagged_as_unlisted() {
    let root = fixture_path("path-aliases");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unlisted_names: Vec<&str> = results
        .unlisted_dependencies
        .iter()
        .map(|d| d.package_name.as_str())
        .collect();

    // @/utils is a path alias, not an npm package, so it should NOT be flagged
    assert!(
        !unlisted_names.contains(&"@/utils"),
        "@/utils should not be flagged as unlisted dependency, found: {unlisted_names:?}"
    );
}

#[test]
fn path_aliases_mixed_exports_no_false_positive_unused_files() {
    let root = fixture_path("path-aliases-mixed-exports");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // types.ts and helpers.ts have SOME used exports (imported via @/ path alias)
    // — they should NOT be in unused_files even though they also have unused exports
    assert!(
        !unused_file_names.contains(&"types.ts".to_string()),
        "types.ts has used exports and should not be an unused file: {unused_file_names:?}"
    );
    assert!(
        !unused_file_names.contains(&"helpers.ts".to_string()),
        "helpers.ts has used exports and should not be an unused file: {unused_file_names:?}"
    );

    // orphan.ts is truly unused — no file imports it
    assert!(
        unused_file_names.contains(&"orphan.ts".to_string()),
        "orphan.ts should be detected as unused file: {unused_file_names:?}"
    );

    // Verify unused exports are correctly detected on reachable files
    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();
    assert!(
        unused_export_names.contains(&"unusedExport"),
        "unusedExport should be detected: {unused_export_names:?}"
    );
    assert!(
        unused_export_names.contains(&"unusedHelper"),
        "unusedHelper should be detected: {unused_export_names:?}"
    );
    assert!(
        !unused_export_names.contains(&"usedExport"),
        "usedExport should NOT be in unused exports: {unused_export_names:?}"
    );
    assert!(
        !unused_export_names.contains(&"usedHelper"),
        "usedHelper should NOT be in unused exports: {unused_export_names:?}"
    );
}

// ── CSS/Tailwind ───────────────────────────────────────────────

#[test]
fn css_apply_marks_tailwind_as_used() {
    let root = fixture_path("css-apply-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // tailwindcss should NOT be in unused dependencies (it's used via @apply in styles.css)
    let unused_dep_names: Vec<&str> = results
        .unused_dependencies
        .iter()
        .map(|d| d.package_name.as_str())
        .collect();
    assert!(
        !unused_dep_names.contains(&"tailwindcss"),
        "tailwindcss should not be unused, it's referenced via @apply in CSS: {unused_dep_names:?}"
    );

    // unused.css should be detected as an unused file
    let unused_files: Vec<&str> = results
        .unused_files
        .iter()
        .filter_map(|f| f.path.file_name())
        .filter_map(|f| f.to_str())
        .collect();
    assert!(
        unused_files.contains(&"unused.css"),
        "unused.css should be detected as unused: {unused_files:?}"
    );
}

#[test]
fn vite_aliases_from_config_resolve_internal_modules() {
    let root = fixture_path("vite-alias-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unresolved_specs: Vec<&str> = results
        .unresolved_imports
        .iter()
        .map(|u| u.specifier.as_str())
        .collect();
    assert!(
        !unresolved_specs.contains(&"@/utils/messages"),
        "vite alias import should resolve, found unresolved: {unresolved_specs:?}"
    );

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        !unused_file_names.contains(&"messages.ts".to_string()),
        "messages.ts should be reachable via vite alias import: {unused_file_names:?}"
    );

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();
    assert!(
        unused_export_names.contains(&"unusedMessage"),
        "reachable aliased module should still report unused exports: {unused_export_names:?}"
    );
}

#[test]
fn sveltekit_aliases_from_config_resolve_internal_modules() {
    let root = fixture_path("sveltekit-alias-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unresolved_specs: Vec<&str> = results
        .unresolved_imports
        .iter()
        .map(|u| u.specifier.as_str())
        .collect();
    assert!(
        !unresolved_specs.contains(&"$utils/greeting"),
        "sveltekit alias import should resolve, found unresolved: {unresolved_specs:?}"
    );

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        !unused_file_names.contains(&"greeting.ts".to_string()),
        "greeting.ts should be reachable via sveltekit alias import: {unused_file_names:?}"
    );

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();
    assert!(
        unused_export_names.contains(&"unusedGreeting"),
        "reachable aliased module should still report unused exports: {unused_export_names:?}"
    );
}

#[test]
fn nuxt_custom_dirs_and_aliases_reduce_false_positives() {
    let root = fixture_path("nuxt-custom-dirs");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unresolved_specs: Vec<&str> = results
        .unresolved_imports
        .iter()
        .map(|u| u.specifier.as_str())
        .collect();
    assert!(
        !unresolved_specs.contains(&"@shared/utils"),
        "nuxt alias import should resolve, found unresolved: {unresolved_specs:?}"
    );

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        !unused_file_names.contains(&"utils.ts".to_string()),
        "utils.ts should be reachable via nuxt alias import: {unused_file_names:?}"
    );
    assert!(
        !unused_file_names.contains(&"useGreeting.ts".to_string()),
        "custom nuxt auto-import dir should keep composable alive: {unused_file_names:?}"
    );
    assert!(
        !unused_file_names.contains(&"FancyCard.vue".to_string()),
        "custom nuxt component dir should keep component alive: {unused_file_names:?}"
    );

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();
    assert!(
        unused_export_names.contains(&"unusedShared"),
        "reachable nuxt aliased module should still report unused exports: {unused_export_names:?}"
    );
}

#[test]
fn nuxt_src_dir_config_reduces_false_positives() {
    let root = fixture_path("nuxt-src-dir");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unresolved_specs: Vec<&str> = results
        .unresolved_imports
        .iter()
        .map(|u| u.specifier.as_str())
        .collect();
    assert!(
        !unresolved_specs.contains(&"@shared/utils"),
        "nuxt srcDir alias import should resolve, found unresolved: {unresolved_specs:?}"
    );

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    for expected_used in [
        "utils.ts",
        "useGreeting.ts",
        "FancyCard.vue",
        "app.vue",
        "app.config.ts",
        "error.vue",
    ] {
        assert!(
            !unused_file_names.contains(&expected_used.to_string()),
            "{expected_used} should be kept alive by Nuxt srcDir support: {unused_file_names:?}"
        );
    }

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();
    assert!(
        unused_export_names.contains(&"unusedShared"),
        "reachable nuxt srcDir aliased module should still report unused exports: {unused_export_names:?}"
    );
}
