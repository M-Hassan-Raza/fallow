//! `TanStack` Router plugin.
//!
//! Detects `TanStack` Router projects and marks route files as entry points.
//! Parses `tsr.config.json` to support custom route directories and generated
//! route-tree locations.

use std::path::Path;

use regex::escape as regex_escape;

use super::{PathRule, Plugin, PluginResult, UsedExportRule, config_parser};

const ENABLERS: &[&str] = &[
    "@tanstack/react-router",
    "@tanstack/solid-router",
    "@tanstack/start",
    "@tanstack/react-start",
    "@tanstack/solid-start",
];

const ENTRY_PATTERNS: &[&str] = &[
    "src/routes/**/*.{ts,tsx,js,jsx}",
    "app/routes/**/*.{ts,tsx,js,jsx}",
    "src/routeTree.gen.ts",
    "src/routeTree.gen.js",
    "app/routeTree.gen.ts",
    "app/routeTree.gen.js",
    "src/server.{ts,tsx,js,jsx}",
    "src/client.{ts,tsx,js,jsx}",
    "src/router.{ts,tsx,js,jsx}",
    "app/server.{ts,tsx,js,jsx}",
    "app/client.{ts,tsx,js,jsx}",
    "app/router.{ts,tsx,js,jsx}",
    "src/routes/__root.{ts,tsx,js,jsx}",
    "app/routes/__root.{ts,tsx,js,jsx}",
];

const CONFIG_PATTERNS: &[&str] = &["tsr.config.json"];

const ALWAYS_USED: &[&str] = &["tsr.config.json", "app.config.{ts,js}"];

const TOOLING_DEPENDENCIES: &[&str] = &[
    "@tanstack/react-router",
    "@tanstack/react-router-devtools",
    "@tanstack/solid-router",
    "@tanstack/solid-router-devtools",
    "@tanstack/start",
    "@tanstack/react-start",
    "@tanstack/solid-start",
    "@tanstack/router-cli",
    "@tanstack/router-vite-plugin",
];

const ROUTE_EXPORTS: &[&str] = &[
    "default",
    "Route",
    "loader",
    "action",
    "component",
    "errorComponent",
    "pendingComponent",
    "notFoundComponent",
    "beforeLoad",
];
const LAZY_ROUTE_EXPORTS: &[&str] = &[
    "Route",
    "component",
    "errorComponent",
    "pendingComponent",
    "notFoundComponent",
];
const DEFAULT_ROUTE_FILE_IGNORE_PREFIX: &str = "-";

pub struct TanstackRouterPlugin;

impl Plugin for TanstackRouterPlugin {
    fn name(&self) -> &'static str {
        "tanstack-router"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn entry_patterns(&self) -> &'static [&'static str] {
        ENTRY_PATTERNS
    }

    fn entry_pattern_rules(&self) -> Vec<PathRule> {
        vec![
            route_dir_entry_rule("src/routes", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None),
            route_dir_entry_rule("app/routes", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None),
            PathRule::from_static("src/routeTree.gen.ts"),
            PathRule::from_static("src/routeTree.gen.js"),
            PathRule::from_static("app/routeTree.gen.ts"),
            PathRule::from_static("app/routeTree.gen.js"),
            PathRule::from_static("src/server.{ts,tsx,js,jsx}"),
            PathRule::from_static("src/client.{ts,tsx,js,jsx}"),
            PathRule::from_static("src/router.{ts,tsx,js,jsx}"),
            PathRule::from_static("app/server.{ts,tsx,js,jsx}"),
            PathRule::from_static("app/client.{ts,tsx,js,jsx}"),
            PathRule::from_static("app/router.{ts,tsx,js,jsx}"),
            PathRule::from_static("src/routes/__root.{ts,tsx,js,jsx}"),
            PathRule::from_static("app/routes/__root.{ts,tsx,js,jsx}"),
        ]
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

    fn used_exports(&self) -> Vec<(&'static str, &'static [&'static str])> {
        vec![
            ("src/routes/**/*.{ts,tsx,js,jsx}", ROUTE_EXPORTS),
            ("app/routes/**/*.{ts,tsx,js,jsx}", ROUTE_EXPORTS),
            ("src/routes/**/*.lazy.{ts,tsx,js,jsx}", LAZY_ROUTE_EXPORTS),
            ("app/routes/**/*.lazy.{ts,tsx,js,jsx}", LAZY_ROUTE_EXPORTS),
        ]
    }

    fn used_export_rules(&self) -> Vec<UsedExportRule> {
        vec![
            route_dir_used_export_rule("src/routes", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None),
            route_dir_used_export_rule("app/routes", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None),
            lazy_route_rule("src/routes", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None),
            lazy_route_rule("app/routes", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None),
        ]
    }

    fn resolve_config(&self, config_path: &Path, source: &str, root: &Path) -> PluginResult {
        let mut result = PluginResult {
            replace_entry_patterns: true,
            ..PluginResult::default()
        };

        let route_dir =
            config_parser::extract_config_string(source, config_path, &["routesDirectory"])
                .as_deref()
                .and_then(|raw| config_parser::normalize_config_path(raw, config_path, root))
                .unwrap_or_else(|| "src/routes".to_string());
        let route_file_ignore_prefix =
            config_parser::extract_config_string(source, config_path, &["routeFileIgnorePrefix"])
                .unwrap_or_else(|| DEFAULT_ROUTE_FILE_IGNORE_PREFIX.to_string());
        let route_file_ignore_pattern =
            config_parser::extract_config_string(source, config_path, &["routeFileIgnorePattern"]);

        add_route_dir_patterns(
            &mut result,
            &route_dir,
            &route_file_ignore_prefix,
            route_file_ignore_pattern.as_deref(),
        );

        let generated_route_tree =
            config_parser::extract_config_string(source, config_path, &["generatedRouteTree"])
                .as_deref()
                .and_then(|raw| config_parser::normalize_config_path(raw, config_path, root));
        if let Some(route_tree) = generated_route_tree {
            result.push_entry_pattern(route_tree);
        } else {
            result.extend_entry_patterns(default_generated_route_tree_paths(&route_dir));
        }

        let base_dir = route_dir_base_dir(&route_dir);
        for name in ["server", "client", "router"] {
            result.push_entry_pattern(format!("{base_dir}/{name}.{{ts,tsx,js,jsx}}"));
        }

        result
    }
}

fn add_route_dir_patterns(
    result: &mut PluginResult,
    route_dir: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) {
    result.entry_patterns.push(route_dir_entry_rule(
        route_dir,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    ));
    result.used_exports.push(route_dir_used_export_rule(
        route_dir,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    ));
    result.used_exports.push(lazy_route_rule(
        route_dir,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    ));
}

fn route_dir_entry_rule(
    route_dir: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) -> PathRule {
    let (exclude_globs, exclude_regexes) = route_dir_exclusions(
        route_dir,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    );
    PathRule::new(format!("{route_dir}/**/*.{{ts,tsx,js,jsx}}"))
        .with_excluded_globs(exclude_globs)
        .with_excluded_regexes(exclude_regexes)
}

fn route_dir_used_export_rule(
    route_dir: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) -> UsedExportRule {
    let (exclude_globs, exclude_regexes) = route_dir_exclusions(
        route_dir,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    );
    UsedExportRule::new(
        format!("{route_dir}/**/*.{{ts,tsx,js,jsx}}"),
        ROUTE_EXPORTS.iter().copied(),
    )
    .with_excluded_globs(exclude_globs)
    .with_excluded_regexes(exclude_regexes)
}

fn lazy_route_rule(
    route_dir: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) -> UsedExportRule {
    let (exclude_globs, exclude_regexes) = route_dir_exclusions(
        route_dir,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    );
    UsedExportRule::new(
        format!("{route_dir}/**/*.lazy.{{ts,tsx,js,jsx}}"),
        LAZY_ROUTE_EXPORTS.iter().copied(),
    )
    .with_excluded_globs(exclude_globs)
    .with_excluded_regexes(exclude_regexes)
}

fn route_dir_exclusions(
    route_dir: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let mut exclude_globs = Vec::new();
    let exclude_regexes = route_file_ignore_pattern
        .map(|pattern| vec![ignored_route_segment_regex(route_dir, pattern)])
        .unwrap_or_default();

    if !route_file_ignore_prefix.is_empty() {
        exclude_globs.push(format!("{route_dir}/**/{route_file_ignore_prefix}*"));
        exclude_globs.push(format!("{route_dir}/**/{route_file_ignore_prefix}*/**/*"));
    }

    (exclude_globs, exclude_regexes)
}

fn ignored_route_segment_regex(route_dir: &str, segment_pattern: &str) -> String {
    format!(
        r"^{}/(?:.*/)?[^/]*(?:{})[^/]*(?:/|$)",
        regex_escape(route_dir),
        segment_pattern
    )
}

fn route_dir_base_dir(route_dir: &str) -> String {
    Path::new(route_dir)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| "src".to_string())
}

fn default_generated_route_tree_paths(route_dir: &str) -> Vec<String> {
    let base_dir = route_dir_base_dir(route_dir);
    vec![
        format!("{base_dir}/routeTree.gen.ts"),
        format!("{base_dir}/routeTree.gen.js"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_exports_cover_lazy_routes() {
        let plugin = TanstackRouterPlugin;
        let exports = plugin.used_export_rules();

        assert!(exports.iter().any(|rule| {
            rule.path.pattern == "src/routes/**/*.lazy.{ts,tsx,js,jsx}"
                && rule.exports.contains(&"Route".to_string())
                && rule.exports.contains(&"component".to_string())
        }));
    }

    #[test]
    fn resolve_config_uses_custom_routes_directory() {
        let plugin = TanstackRouterPlugin;
        let result = plugin.resolve_config(
            Path::new("/project/tsr.config.json"),
            r#"{
                "routesDirectory": "./app/pages",
                "generatedRouteTree": "./app/routeTree.gen.ts",
                "routeFileIgnorePrefix": "-"
            }"#,
            Path::new("/project"),
        );

        assert!(result.replace_entry_patterns);
        assert!(
            result
                .entry_patterns
                .iter()
                .any(|rule| rule.pattern == "app/pages/**/*.{ts,tsx,js,jsx}"),
            "entry patterns: {:?}",
            result.entry_patterns
        );
        assert!(
            result
                .entry_patterns
                .iter()
                .any(|rule| rule.pattern == "app/routeTree.gen.ts")
        );
        assert!(
            result
                .entry_patterns
                .iter()
                .any(|rule| rule.pattern == "app/router.{ts,tsx,js,jsx}")
        );
    }

    #[test]
    fn route_rules_exclude_ignored_prefixes_and_patterns() {
        let route_rule = route_dir_used_export_rule("app/pages", "-", Some("test-page"));

        assert!(
            route_rule
                .path
                .exclude_globs
                .contains(&"app/pages/**/-*".to_string())
        );
        assert_eq!(route_rule.path.exclude_regexes.len(), 1);
        assert!(route_rule.path.exclude_regexes[0].contains("test-page"));
    }
}
