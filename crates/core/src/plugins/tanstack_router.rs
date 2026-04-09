//! `TanStack` Router plugin.
//!
//! Detects `TanStack` Router projects and marks route files as entry points.
//! Parses `tsr.config.json` to support custom route directories and generated
//! route-tree locations.

use std::path::Path;

use super::{PathRule, Plugin, PluginResult, UsedExportRule, config_parser};

const ENABLERS: &[&str] = &[
    "@tanstack/react-router",
    "@tanstack/solid-router",
    "@tanstack/start",
    "@tanstack/react-start",
    "@tanstack/solid-start",
];

const DEFAULT_ROUTE_DIRS: &[&str] = &["src/routes", "app/routes"];
const SUPPORTING_ENTRY_PATTERNS: &[&str] = &[
    "src/server.{ts,tsx,js,jsx}",
    "src/client.{ts,tsx,js,jsx}",
    "src/router.{ts,tsx,js,jsx}",
];
const DEFAULT_GENERATED_ROUTE_TREE_PATTERNS: &[&str] =
    &["src/routeTree.gen.ts", "src/routeTree.gen.js"];
const ENTRY_PATTERNS: &[&str] = &[
    "src/routes/**/*.{ts,tsx,js,jsx}",
    "app/routes/**/*.{ts,tsx,js,jsx}",
    "src/server.{ts,tsx,js,jsx}",
    "src/client.{ts,tsx,js,jsx}",
    "src/router.{ts,tsx,js,jsx}",
    "src/routeTree.gen.ts",
    "src/routeTree.gen.js",
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
const ROUTE_FILE_EXTENSIONS: &str = "{ts,tsx,js,jsx}";

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
        let mut rules = DEFAULT_ROUTE_DIRS
            .iter()
            .flat_map(|route_dir| {
                [
                    route_dir_rule(
                        route_dir,
                        "",
                        DEFAULT_ROUTE_FILE_IGNORE_PREFIX,
                        None,
                        RouteFileKind::Standard,
                    ),
                    route_dir_rule(
                        route_dir,
                        "",
                        DEFAULT_ROUTE_FILE_IGNORE_PREFIX,
                        None,
                        RouteFileKind::Lazy,
                    ),
                ]
            })
            .collect::<Vec<_>>();
        rules.extend(
            DEFAULT_GENERATED_ROUTE_TREE_PATTERNS
                .iter()
                .chain(SUPPORTING_ENTRY_PATTERNS.iter())
                .map(|pattern| PathRule::from_static(pattern)),
        );
        rules
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
        DEFAULT_ROUTE_DIRS
            .iter()
            .flat_map(|route_dir| {
                [
                    route_dir_used_export_rule(
                        route_dir,
                        "",
                        DEFAULT_ROUTE_FILE_IGNORE_PREFIX,
                        None,
                    ),
                    lazy_route_rule(route_dir, "", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None),
                ]
            })
            .collect()
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
        let route_file_prefix =
            config_parser::extract_config_string(source, config_path, &["routeFilePrefix"])
                .unwrap_or_default();
        let route_file_ignore_prefix =
            config_parser::extract_config_string(source, config_path, &["routeFileIgnorePrefix"])
                .unwrap_or_else(|| DEFAULT_ROUTE_FILE_IGNORE_PREFIX.to_string());
        let route_file_ignore_pattern =
            config_parser::extract_config_string(source, config_path, &["routeFileIgnorePattern"]);

        add_route_dir_patterns(
            &mut result,
            &route_dir,
            &route_file_prefix,
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
            result.extend_entry_patterns(DEFAULT_GENERATED_ROUTE_TREE_PATTERNS.iter().copied());
        }
        result.extend_entry_patterns(SUPPORTING_ENTRY_PATTERNS.iter().copied());

        result
    }
}

fn add_route_dir_patterns(
    result: &mut PluginResult,
    route_dir: &str,
    route_file_prefix: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) {
    result.entry_patterns.push(route_dir_rule(
        route_dir,
        route_file_prefix,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
        RouteFileKind::Standard,
    ));
    result.entry_patterns.push(route_dir_rule(
        route_dir,
        route_file_prefix,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
        RouteFileKind::Lazy,
    ));
    result.used_exports.push(route_dir_used_export_rule(
        route_dir,
        route_file_prefix,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    ));
    result.used_exports.push(lazy_route_rule(
        route_dir,
        route_file_prefix,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    ));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RouteFileKind {
    Standard,
    Lazy,
}

#[derive(Default)]
struct RouteDirExclusions {
    globs: Vec<String>,
    segment_regexes: Vec<String>,
}

fn route_dir_rule(
    route_dir: &str,
    route_file_prefix: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
    file_kind: RouteFileKind,
) -> PathRule {
    let mut exclusions = route_dir_exclusions(
        route_dir,
        route_file_ignore_prefix,
        route_file_ignore_pattern,
    );
    if file_kind == RouteFileKind::Standard {
        exclusions.globs.push(route_file_pattern(
            route_dir,
            route_file_prefix,
            RouteFileKind::Lazy,
        ));
    }

    PathRule::new(route_file_pattern(route_dir, route_file_prefix, file_kind))
        .with_excluded_globs(exclusions.globs)
        .with_excluded_segment_regexes(exclusions.segment_regexes)
}

fn route_dir_used_export_rule(
    route_dir: &str,
    route_file_prefix: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) -> UsedExportRule {
    used_export_rule_from_path_rule(
        route_dir_rule(
            route_dir,
            route_file_prefix,
            route_file_ignore_prefix,
            route_file_ignore_pattern,
            RouteFileKind::Standard,
        ),
        ROUTE_EXPORTS,
    )
}

fn lazy_route_rule(
    route_dir: &str,
    route_file_prefix: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) -> UsedExportRule {
    used_export_rule_from_path_rule(
        route_dir_rule(
            route_dir,
            route_file_prefix,
            route_file_ignore_prefix,
            route_file_ignore_pattern,
            RouteFileKind::Lazy,
        ),
        LAZY_ROUTE_EXPORTS,
    )
}

fn route_dir_exclusions(
    route_dir: &str,
    route_file_ignore_prefix: &str,
    route_file_ignore_pattern: Option<&str>,
) -> RouteDirExclusions {
    let mut exclusions = RouteDirExclusions::default();

    if !route_file_ignore_prefix.is_empty() {
        exclusions
            .globs
            .push(format!("{route_dir}/**/{route_file_ignore_prefix}*"));
        exclusions
            .globs
            .push(format!("{route_dir}/**/{route_file_ignore_prefix}*/**/*"));
    }

    if let Some(pattern) = route_file_ignore_pattern {
        exclusions.segment_regexes.push(pattern.to_string());
    }

    exclusions
}

fn route_file_pattern(
    route_dir: &str,
    route_file_prefix: &str,
    file_kind: RouteFileKind,
) -> String {
    match file_kind {
        RouteFileKind::Standard => {
            format!("{route_dir}/**/{route_file_prefix}*.{ROUTE_FILE_EXTENSIONS}")
        }
        RouteFileKind::Lazy => {
            format!("{route_dir}/**/{route_file_prefix}*.lazy.{ROUTE_FILE_EXTENSIONS}")
        }
    }
}

fn used_export_rule_from_path_rule(
    rule: PathRule,
    exports: &'static [&'static str],
) -> UsedExportRule {
    UsedExportRule::new(rule.pattern, exports.iter().copied())
        .with_excluded_globs(rule.exclude_globs)
        .with_excluded_regexes(rule.exclude_regexes)
        .with_excluded_segment_regexes(rule.exclude_segment_regexes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_exports_cover_lazy_routes_without_inheriting_non_lazy_exports() {
        let lazy_rule = lazy_route_rule("src/routes", "", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None);
        let broad_rule =
            route_dir_used_export_rule("src/routes", "", DEFAULT_ROUTE_FILE_IGNORE_PREFIX, None);

        assert_eq!(
            lazy_rule.path.pattern,
            "src/routes/**/*.lazy.{ts,tsx,js,jsx}"
        );
        assert!(lazy_rule.exports.contains(&"Route".to_string()));
        assert!(lazy_rule.exports.contains(&"component".to_string()));
        assert!(
            broad_rule
                .path
                .exclude_globs
                .contains(&"src/routes/**/*.lazy.{ts,tsx,js,jsx}".to_string())
        );
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
                .any(|rule| rule.pattern == "src/router.{ts,tsx,js,jsx}")
        );
    }

    #[test]
    fn resolve_config_keeps_default_supporting_entries_with_custom_route_dir() {
        let plugin = TanstackRouterPlugin;
        let result = plugin.resolve_config(
            Path::new("/project/tsr.config.json"),
            r#"{
                "routesDirectory": "./app/pages"
            }"#,
            Path::new("/project"),
        );

        for expected in [
            "app/pages/**/*.{ts,tsx,js,jsx}",
            "src/routeTree.gen.ts",
            "src/routeTree.gen.js",
            "src/server.{ts,tsx,js,jsx}",
            "src/client.{ts,tsx,js,jsx}",
            "src/router.{ts,tsx,js,jsx}",
        ] {
            assert!(
                result
                    .entry_patterns
                    .iter()
                    .any(|rule| rule.pattern == expected),
                "missing supporting entry pattern {expected}: {:?}",
                result.entry_patterns
            );
        }
    }

    #[test]
    fn route_rules_honor_route_file_prefix() {
        let route_rule = route_dir_used_export_rule(
            "app/pages",
            "route-",
            DEFAULT_ROUTE_FILE_IGNORE_PREFIX,
            None,
        );

        assert_eq!(
            route_rule.path.pattern,
            "app/pages/**/route-*.{ts,tsx,js,jsx}"
        );
        assert!(
            route_rule
                .path
                .exclude_globs
                .contains(&"app/pages/**/route-*.lazy.{ts,tsx,js,jsx}".to_string())
        );
    }

    #[test]
    fn route_rules_preserve_segment_ignore_regexes() {
        let route_rule = route_dir_used_export_rule(
            "app/pages",
            "",
            DEFAULT_ROUTE_FILE_IGNORE_PREFIX,
            Some("^ignored\\."),
        );

        assert!(
            route_rule
                .path
                .exclude_globs
                .contains(&"app/pages/**/-*".to_string())
        );
        assert_eq!(
            route_rule.path.exclude_segment_regexes,
            vec!["^ignored\\.".to_string()]
        );
        assert!(route_rule.path.exclude_regexes.is_empty());
    }
}
