//! Expo Router framework plugin.
//!
//! Detects Expo Router projects, discovers the configured route root from app
//! config, and marks route files plus special-file exports as framework-used.

use std::path::Path;

use super::{PathRule, Plugin, PluginResult, UsedExportRule, config_parser};

const ENABLERS: &[&str] = &["expo-router"];

const CONFIG_PATTERNS: &[&str] = &["app.json", "app.config.{ts,js,mjs,cjs}"];

const ALWAYS_USED: &[&str] = &[
    "app.json",
    "app.config.{ts,js,mjs,cjs}",
    "metro.config.{ts,js,mjs,cjs}",
    "babel.config.{ts,js,mjs,cjs}",
    "expo-env.d.ts",
];

const TOOLING_DEPENDENCIES: &[&str] = &[
    "expo",
    "expo-router",
    "expo-linking",
    "expo-server",
    "@expo/metro-runtime",
];

const ROUTE_FILE_EXPORTS: &[&str] = &[
    "default",
    "ErrorBoundary",
    "loader",
    "generateStaticParams",
    "unstable_settings",
];
const API_ROUTE_EXPORTS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const NATIVE_INTENT_EXPORTS: &[&str] = &["redirectSystemPath", "legacy_subscribe"];
const MIDDLEWARE_EXPORTS: &[&str] = &["default", "unstable_settings"];
const DEFAULT_EXPORTS: &[&str] = &["default"];

pub struct ExpoRouterPlugin;

impl Plugin for ExpoRouterPlugin {
    fn name(&self) -> &'static str {
        "expo-router"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn config_patterns(&self) -> &'static [&'static str] {
        CONFIG_PATTERNS
    }

    fn always_used(&self) -> &'static [&'static str] {
        ALWAYS_USED
    }

    fn tooling_dependencies(&self) -> &'static [&'static str] {
        TOOLING_DEPENDENCIES
    }

    fn resolve_config(&self, config_path: &Path, source: &str, root: &Path) -> PluginResult {
        let mut result = PluginResult::default();

        for import in config_parser::extract_imports(source, config_path) {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(&import));
        }

        let route_root = extract_route_root(source, config_path, root)
            .unwrap_or_else(|| default_route_root(root).to_string());
        add_route_root_patterns(&mut result, &route_root);

        result
    }
}

fn extract_route_root(source: &str, config_path: &Path, root: &Path) -> Option<String> {
    let raw = config_parser::extract_config_plugin_option_string_from_paths(
        source,
        config_path,
        &[&["plugins"], &["expo", "plugins"]],
        "expo-router",
        "root",
    )?;

    config_parser::normalize_config_path(raw.trim(), config_path, root)
}

fn default_route_root(root: &Path) -> &'static str {
    if root.join("src/app").is_dir() {
        "src/app"
    } else {
        "app"
    }
}

fn add_route_root_patterns(result: &mut PluginResult, route_root: &str) {
    let route_pattern = format!("{route_root}/**/*.{{ts,tsx,js,jsx}}");
    let special_patterns = special_route_patterns(route_root);

    result
        .entry_patterns
        .push(PathRule::new(route_pattern.clone()));
    result.used_exports.push(
        UsedExportRule::new(route_pattern, ROUTE_FILE_EXPORTS.iter().copied())
            .with_excluded_globs(special_patterns.iter().map(|(pattern, _)| pattern.clone())),
    );

    for (pattern, exports) in special_patterns {
        result
            .used_exports
            .push(UsedExportRule::new(pattern, exports.iter().copied()));
    }
}

fn special_route_patterns(route_root: &str) -> Vec<(String, &'static [&'static str])> {
    vec![
        (
            format!("{route_root}/**/*+api.{{ts,tsx,js,jsx}}"),
            API_ROUTE_EXPORTS,
        ),
        (
            format!("{route_root}/**/+native-intent.{{ts,tsx,js,jsx}}"),
            NATIVE_INTENT_EXPORTS,
        ),
        (
            format!("{route_root}/**/+middleware.{{ts,tsx,js,jsx}}"),
            MIDDLEWARE_EXPORTS,
        ),
        (
            format!("{route_root}/**/+html.{{ts,tsx,js,jsx}}"),
            DEFAULT_EXPORTS,
        ),
        (
            format!("{route_root}/**/+not-found.{{ts,tsx,js,jsx}}"),
            DEFAULT_EXPORTS,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn has_entry_pattern(result: &PluginResult, pattern: &str) -> bool {
        result
            .entry_patterns
            .iter()
            .any(|entry_pattern| entry_pattern.pattern == pattern)
    }

    #[test]
    fn resolve_config_uses_custom_route_root() {
        let plugin = ExpoRouterPlugin;
        let source = r#"{
            "plugins": [
                ["expo-router", { "root": "src/app" }]
            ]
        }"#;

        let result = plugin.resolve_config(
            Path::new("/project/app.json"),
            source,
            Path::new("/project"),
        );

        assert!(
            has_entry_pattern(&result, "src/app/**/*.{ts,tsx,js,jsx}"),
            "entry patterns: {:?}",
            result.entry_patterns
        );
        assert!(result.used_exports.iter().any(|rule| {
            rule.path.pattern == "src/app/**/*.{ts,tsx,js,jsx}"
                && rule.exports.iter().any(|export| export == "loader")
        }));
    }

    #[test]
    fn resolve_config_prefers_src_app_even_when_root_app_exists() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src/app")).unwrap();
        fs::create_dir_all(temp.path().join("app")).unwrap();

        let plugin = ExpoRouterPlugin;
        let result = plugin.resolve_config(
            temp.path().join("app.json").as_path(),
            r#"{"expo":{"name":"demo"}}"#,
            temp.path(),
        );

        assert!(
            has_entry_pattern(&result, "src/app/**/*.{ts,tsx,js,jsx}"),
            "entry patterns: {:?}",
            result.entry_patterns
        );
    }

    #[test]
    fn route_rules_exclude_special_files_from_generic_exports() {
        let mut result = PluginResult::default();
        add_route_root_patterns(&mut result, "src/app");

        let generic_rule = result
            .used_exports
            .iter()
            .find(|rule| rule.path.pattern == "src/app/**/*.{ts,tsx,js,jsx}")
            .expect("missing generic route rule");

        assert!(
            generic_rule
                .path
                .exclude_globs
                .iter()
                .any(|pattern| pattern == "src/app/**/*+api.{ts,tsx,js,jsx}")
        );
    }
}
