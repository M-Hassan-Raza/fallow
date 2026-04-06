//! Nuxt framework plugin.
//!
//! Detects Nuxt projects and marks pages, layouts, middleware, server API,
//! plugins, composables, and utils as entry points. Recognizes conventional
//! server API and middleware exports. Parses nuxt.config.ts to extract modules,
//! CSS files, plugins, and other configuration.

use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

const ENABLERS: &[&str] = &["nuxt"];

const ENTRY_PATTERNS: &[&str] = &[
    // Standard Nuxt directories
    "pages/**/*.{vue,ts,tsx,js,jsx}",
    "layouts/**/*.{vue,ts,tsx,js,jsx}",
    "middleware/**/*.{ts,js}",
    "server/api/**/*.{ts,js}",
    "server/routes/**/*.{ts,js}",
    "server/middleware/**/*.{ts,js}",
    "server/utils/**/*.{ts,js}",
    "plugins/**/*.{ts,js}",
    "composables/**/*.{ts,js}",
    "utils/**/*.{ts,js}",
    "components/**/*.{vue,ts,tsx,js,jsx}",
    // Nuxt auto-scans modules/ for custom modules
    "modules/**/*.{ts,js}",
    // Nuxt 3 app/ directory structure
    "app/pages/**/*.{vue,ts,tsx,js,jsx}",
    "app/layouts/**/*.{vue,ts,tsx,js,jsx}",
    "app/middleware/**/*.{ts,js}",
    "app/plugins/**/*.{ts,js}",
    "app/composables/**/*.{ts,js}",
    "app/utils/**/*.{ts,js}",
    "app/components/**/*.{vue,ts,tsx,js,jsx}",
    "app/modules/**/*.{ts,js}",
];

const SRC_DIR_ENTRY_PATTERNS: &[&str] = &[
    "pages/**/*.{vue,ts,tsx,js,jsx}",
    "layouts/**/*.{vue,ts,tsx,js,jsx}",
    "middleware/**/*.{ts,js}",
    "plugins/**/*.{ts,js}",
    "composables/**/*.{ts,js}",
    "utils/**/*.{ts,js}",
    "components/**/*.{vue,ts,tsx,js,jsx}",
];

const CONFIG_PATTERNS: &[&str] = &["nuxt.config.{ts,js}"];

const ALWAYS_USED: &[&str] = &[
    "nuxt.config.{ts,js}",
    "app.vue",
    "app.config.{ts,js}",
    "error.vue",
    // Nuxt 3 app/ directory structure
    "app/app.vue",
    "app/app.config.{ts,js}",
    "app/error.vue",
];

const SRC_DIR_ALWAYS_USED: &[&str] = &["app.vue", "app.config.{ts,js}", "error.vue"];

/// Implicit dependencies that Nuxt provides — these should not be flagged as unlisted.
const TOOLING_DEPENDENCIES: &[&str] = &[
    "nuxt",
    "@nuxt/devtools",
    "@nuxt/test-utils",
    "@nuxt/schema",
    "@nuxt/kit",
    // Implicit Nuxt runtime dependencies (re-exported by Nuxt at build time)
    "vue",
    "vue-router",
    "ofetch",
    "h3",
    "@unhead/vue",
    "@unhead/schema",
    "nitropack",
    "defu",
    "hookable",
    "ufo",
    "unctx",
    "unenv",
    "ohash",
    "pathe",
    "scule",
    "unimport",
    "unstorage",
    "radix3",
    "cookie-es",
    "crossws",
    "consola",
];

const USED_EXPORTS_SERVER_API: &[&str] = &["default", "defineEventHandler"];
const USED_EXPORTS_MIDDLEWARE: &[&str] = &["default"];

/// Virtual module prefixes provided by Nuxt at build time.
const VIRTUAL_MODULE_PREFIXES: &[&str] = &["#"];

pub struct NuxtPlugin;

impl Plugin for NuxtPlugin {
    fn name(&self) -> &'static str {
        "nuxt"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn entry_patterns(&self) -> &'static [&'static str] {
        ENTRY_PATTERNS
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

    fn virtual_module_prefixes(&self) -> &'static [&'static str] {
        VIRTUAL_MODULE_PREFIXES
    }

    fn path_aliases(&self, root: &Path) -> Vec<(&'static str, String)> {
        // Nuxt's srcDir defaults to `app/` when the directory exists, otherwise root.
        let src_dir = if root.join("app").is_dir() {
            "app".to_string()
        } else {
            String::new()
        };
        let mut aliases = vec![
            // ~/  → srcDir (app/ or root)
            ("~/", src_dir.clone()),
            // @/  → srcDir (Nuxt alias synonym for ~/)
            ("@/", src_dir),
            // ~~/ → rootDir (project root)
            ("~~/", String::new()),
            // @@/ → rootDir (Nuxt alias synonym for ~~/)
            ("@@/", String::new()),
            // #shared/ → shared/ directory
            ("#shared/", "shared".to_string()),
            // #server/ → server/ directory
            ("#server/", "server".to_string()),
        ];
        // Also map the bare `~` and `~~` (without trailing slash) for edge cases
        // like `import '~/composables/foo'` — already covered by `~/` prefix.
        // Map #shared (without slash) for bare imports like `import '#shared'`
        aliases.push(("#shared", "shared".to_string()));
        aliases.push(("#server", "server".to_string()));
        aliases
    }

    fn used_exports(&self) -> Vec<(&'static str, &'static [&'static str])> {
        vec![
            ("server/api/**/*.{ts,js}", USED_EXPORTS_SERVER_API),
            ("middleware/**/*.{ts,js}", USED_EXPORTS_MIDDLEWARE),
        ]
    }

    fn resolve_config(&self, config_path: &Path, source: &str, root: &Path) -> PluginResult {
        let mut result = PluginResult::default();

        // Nuxt aliases resolve against srcDir, which defaults to `app/` when it exists
        // and can be overridden explicitly via config.
        let default_src_dir = default_nuxt_src_dir(root);
        let configured_src_dir = extract_nuxt_src_dir(source, config_path, root);
        let src_dir = configured_src_dir
            .clone()
            .unwrap_or_else(|| default_src_dir.clone());

        if let Some(configured_src_dir) = configured_src_dir.as_deref()
            && configured_src_dir != default_src_dir.as_str()
        {
            add_src_dir_support(&mut result, configured_src_dir);
        }

        // Extract import sources as referenced dependencies
        let imports = config_parser::extract_imports(source, config_path);
        for imp in &imports {
            let dep = crate::resolve::extract_package_name(imp);
            result.referenced_dependencies.push(dep);
        }

        // modules: [...] → referenced dependencies (Nuxt modules are npm packages)
        let modules = config_parser::extract_config_string_array(source, config_path, &["modules"]);
        for module in &modules {
            let dep = crate::resolve::extract_package_name(module);
            result.referenced_dependencies.push(dep);
        }

        // css: [...] → always-used files or referenced dependencies
        // Nuxt aliases: `~/` = srcDir (app/ or root), `~~/` = rootDir
        // npm package CSS (e.g., `@unocss/reset/tailwind.css`) → referenced dependency
        let css = config_parser::extract_config_string_array(source, config_path, &["css"]);
        for entry in &css {
            if let Some(stripped) = entry.strip_prefix("~/") {
                // ~ = srcDir: resolve to the configured source root, if any.
                if src_dir.is_empty() {
                    result.always_used_files.push(stripped.to_string());
                } else {
                    result
                        .always_used_files
                        .push(format!("{src_dir}/{stripped}"));
                }
            } else if let Some(stripped) = entry.strip_prefix("~~/") {
                // ~~ = rootDir: always relative to project root
                result.always_used_files.push(stripped.to_string());
            } else if entry.starts_with('.') || entry.starts_with('/') {
                // Relative or absolute local path
                result.always_used_files.push(entry.clone());
            } else {
                // npm package CSS (e.g., `@unocss/reset/tailwind.css`, `floating-vue/dist/style.css`)
                let dep = crate::resolve::extract_package_name(entry);
                result.referenced_dependencies.push(dep);
            }
        }

        // postcss.plugins → referenced dependencies (object keys)
        let postcss_plugins =
            config_parser::extract_config_object_keys(source, config_path, &["postcss", "plugins"]);
        for plugin in &postcss_plugins {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(plugin));
        }

        // plugins: [...] → entry patterns
        let plugins = config_parser::extract_config_string_array(source, config_path, &["plugins"]);
        result.entry_patterns.extend(plugins);

        // alias: { "@shared": "./shared" } → resolver path aliases
        for (find, replacement) in
            config_parser::extract_config_aliases(source, config_path, &["alias"])
        {
            if let Some(normalized) = normalize_nuxt_path(&replacement, config_path, root, &src_dir)
            {
                result.path_aliases.push((find, normalized));
            }
        }

        // imports.dirs: ["~/custom/composables"] → auto-import roots
        for dir in
            config_parser::extract_config_string_array(source, config_path, &["imports", "dirs"])
        {
            if let Some(normalized) = normalize_nuxt_path(&dir, config_path, root, &src_dir) {
                result.entry_patterns.push(format!(
                    "{normalized}/**/*.{{ts,tsx,js,jsx,mts,cts,mjs,cjs}}"
                ));
            }
        }

        // components config supports string arrays, object arrays, and object.dirs arrays.
        let mut component_dirs = config_parser::extract_config_array_object_strings(
            source,
            config_path,
            &["components"],
            "path",
        );
        component_dirs.extend(config_parser::extract_config_string_array(
            source,
            config_path,
            &["components", "dirs"],
        ));
        for dir in component_dirs {
            if let Some(normalized) = normalize_nuxt_path(&dir, config_path, root, &src_dir) {
                result
                    .entry_patterns
                    .push(format!("{normalized}/**/*.{{vue,ts,tsx,js,jsx}}"));
            }
        }

        // extends: [...] → referenced dependencies
        let extends = config_parser::extract_config_string_array(source, config_path, &["extends"]);
        for ext in &extends {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(ext));
        }

        result
    }
}

fn default_nuxt_src_dir(root: &Path) -> String {
    if root.join("app").is_dir() {
        "app".to_string()
    } else {
        String::new()
    }
}

fn extract_nuxt_src_dir(source: &str, config_path: &Path, root: &Path) -> Option<String> {
    let raw = config_parser::extract_config_string(source, config_path, &["srcDir"])?;
    normalize_nuxt_src_dir(&raw, config_path, root)
}

fn normalize_nuxt_src_dir(raw: &str, config_path: &Path, root: &Path) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return Some(String::new());
    }
    config_parser::normalize_config_path(trimmed, config_path, root)
}

fn add_src_dir_support(result: &mut PluginResult, src_dir: &str) {
    result
        .path_aliases
        .push(("~/".to_string(), src_dir.to_string()));
    result
        .path_aliases
        .push(("@/".to_string(), src_dir.to_string()));

    if src_dir.is_empty() {
        return;
    }

    for pattern in SRC_DIR_ENTRY_PATTERNS {
        result.entry_patterns.push(format!("{src_dir}/{pattern}"));
    }

    for pattern in SRC_DIR_ALWAYS_USED {
        result
            .always_used_files
            .push(format!("{src_dir}/{pattern}"));
    }
}

fn normalize_nuxt_path(
    raw: &str,
    config_path: &Path,
    root: &Path,
    src_dir: &str,
) -> Option<String> {
    if let Some(stripped) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("@/")) {
        return Some(if src_dir.is_empty() {
            stripped.to_string()
        } else {
            format!("{src_dir}/{stripped}")
        });
    }

    if let Some(stripped) = raw.strip_prefix("~~/").or_else(|| raw.strip_prefix("@@/")) {
        return Some(stripped.to_string());
    }

    config_parser::normalize_config_path(raw, config_path, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabler_is_nuxt() {
        let plugin = NuxtPlugin;
        assert_eq!(plugin.enablers(), &["nuxt"]);
    }

    #[test]
    fn is_enabled_with_nuxt_dep() {
        let plugin = NuxtPlugin;
        let deps = vec!["nuxt".to_string()];
        assert!(plugin.is_enabled_with_deps(&deps, Path::new("/project")));
    }

    #[test]
    fn is_not_enabled_without_nuxt() {
        let plugin = NuxtPlugin;
        let deps = vec!["vue".to_string()];
        assert!(!plugin.is_enabled_with_deps(&deps, Path::new("/project")));
    }

    #[test]
    fn entry_patterns_include_nuxt_conventions() {
        let plugin = NuxtPlugin;
        let patterns = plugin.entry_patterns();
        assert!(patterns.iter().any(|p| p.starts_with("pages/")));
        assert!(patterns.iter().any(|p| p.starts_with("layouts/")));
        assert!(patterns.iter().any(|p| p.starts_with("server/api/")));
        assert!(patterns.iter().any(|p| p.starts_with("composables/")));
        assert!(patterns.iter().any(|p| p.starts_with("components/")));
    }

    #[test]
    fn entry_patterns_include_app_dir_variants() {
        let plugin = NuxtPlugin;
        let patterns = plugin.entry_patterns();
        assert!(
            patterns.iter().any(|p| p.starts_with("app/pages/")),
            "should include Nuxt 3 app/ directory variants"
        );
    }

    #[test]
    fn virtual_module_prefixes_includes_hash() {
        let plugin = NuxtPlugin;
        assert_eq!(plugin.virtual_module_prefixes(), &["#"]);
    }

    #[test]
    fn path_aliases_include_nuxt_at_variants() {
        let plugin = NuxtPlugin;
        let aliases = plugin.path_aliases(Path::new("/project"));
        assert!(aliases.iter().any(|(prefix, _)| *prefix == "@/"));
        assert!(aliases.iter().any(|(prefix, _)| *prefix == "@@/"));
    }

    #[test]
    fn used_exports_for_server_api() {
        let plugin = NuxtPlugin;
        let exports = plugin.used_exports();
        let api_entry = exports
            .iter()
            .find(|(pat, _)| *pat == "server/api/**/*.{ts,js}");
        assert!(api_entry.is_some());
        let (_, names) = api_entry.unwrap();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"defineEventHandler"));
    }

    // ── resolve_config tests ─────────────────────────────────────

    #[test]
    fn resolve_config_modules_as_deps() {
        let source = r#"
            export default defineNuxtConfig({
                modules: ["@nuxtjs/tailwindcss", "@pinia/nuxt"]
            });
        "#;
        let plugin = NuxtPlugin;
        let result =
            plugin.resolve_config(Path::new("nuxt.config.ts"), source, Path::new("/project"));
        assert!(
            result
                .referenced_dependencies
                .contains(&"@nuxtjs/tailwindcss".to_string())
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"@pinia/nuxt".to_string())
        );
    }

    #[test]
    fn resolve_config_css_tilde_resolves_to_root() {
        // Without an `app/` dir, `~/` resolves to project root
        let source = r#"
            export default defineNuxtConfig({
                css: ["~/assets/main.css"]
            });
        "#;
        let plugin = NuxtPlugin;
        let result = plugin.resolve_config(
            Path::new("nuxt.config.ts"),
            source,
            Path::new("/nonexistent"),
        );
        assert!(
            result
                .always_used_files
                .contains(&"assets/main.css".to_string()),
            "~/assets/main.css should resolve to assets/main.css without app/ dir: {:?}",
            result.always_used_files
        );
    }

    #[test]
    fn resolve_config_css_double_tilde_always_root() {
        let source = r#"
            export default defineNuxtConfig({
                css: ["~~/shared/global.css"]
            });
        "#;
        let plugin = NuxtPlugin;
        let result = plugin.resolve_config(
            Path::new("nuxt.config.ts"),
            source,
            Path::new("/nonexistent"),
        );
        assert!(
            result
                .always_used_files
                .contains(&"shared/global.css".to_string()),
            "~~/shared/global.css should resolve to shared/global.css"
        );
    }

    #[test]
    fn resolve_config_css_npm_package() {
        let source = r#"
            export default defineNuxtConfig({
                css: ["@unocss/reset/tailwind.css"]
            });
        "#;
        let plugin = NuxtPlugin;
        let result =
            plugin.resolve_config(Path::new("nuxt.config.ts"), source, Path::new("/project"));
        assert!(
            result
                .referenced_dependencies
                .contains(&"@unocss/reset".to_string()),
            "npm package CSS should be tracked as referenced dependency"
        );
    }

    #[test]
    fn resolve_config_postcss_plugins_as_deps() {
        let source = r#"
            export default defineNuxtConfig({
                postcss: {
                    plugins: {
                        autoprefixer: {},
                        "postcss-nested": {}
                    }
                }
            });
        "#;
        let plugin = NuxtPlugin;
        let result =
            plugin.resolve_config(Path::new("nuxt.config.ts"), source, Path::new("/project"));
        assert!(
            result
                .referenced_dependencies
                .contains(&"autoprefixer".to_string())
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"postcss-nested".to_string())
        );
    }

    #[test]
    fn resolve_config_extends_as_deps() {
        let source = r#"
            export default defineNuxtConfig({
                extends: ["@nuxt/ui-pro"]
            });
        "#;
        let plugin = NuxtPlugin;
        let result =
            plugin.resolve_config(Path::new("nuxt.config.ts"), source, Path::new("/project"));
        assert!(
            result
                .referenced_dependencies
                .contains(&"@nuxt/ui-pro".to_string())
        );
    }

    #[test]
    fn resolve_config_import_sources_as_deps() {
        let source = r#"
            import { defineNuxtConfig } from "nuxt/config";
            export default defineNuxtConfig({});
        "#;
        let plugin = NuxtPlugin;
        let result =
            plugin.resolve_config(Path::new("nuxt.config.ts"), source, Path::new("/project"));
        assert!(
            result.referenced_dependencies.contains(&"nuxt".to_string()),
            "import source should be extracted as a referenced dependency"
        );
    }

    #[test]
    fn resolve_config_empty_source() {
        let plugin = NuxtPlugin;
        let result = plugin.resolve_config(Path::new("nuxt.config.ts"), "", Path::new("/project"));
        assert!(result.referenced_dependencies.is_empty());
        assert!(result.always_used_files.is_empty());
        assert!(result.entry_patterns.is_empty());
    }

    #[test]
    fn resolve_config_css_relative_path() {
        let source = r#"
            export default defineNuxtConfig({
                css: ["./assets/global.css"]
            });
        "#;
        let plugin = NuxtPlugin;
        let result =
            plugin.resolve_config(Path::new("nuxt.config.ts"), source, Path::new("/project"));
        assert!(
            result
                .always_used_files
                .contains(&"./assets/global.css".to_string()),
            "relative CSS path should be an always-used file"
        );
    }

    #[test]
    fn resolve_config_extracts_custom_aliases_and_dirs() {
        let source = r#"
            export default defineNuxtConfig({
                srcDir: "app/",
                alias: {
                    "@shared": "./app/shared"
                },
                imports: {
                    dirs: ["~/custom/composables"]
                },
                components: [
                    { path: "@/feature-components" }
                ]
            });
        "#;
        let plugin = NuxtPlugin;
        let result = plugin.resolve_config(
            Path::new("/project/nuxt.config.ts"),
            source,
            Path::new("/project"),
        );

        assert!(
            result
                .path_aliases
                .contains(&("@shared".to_string(), "app/shared".to_string()))
        );
        assert!(
            result
                .path_aliases
                .contains(&("~/".to_string(), "app".to_string()))
        );
        assert!(
            result
                .path_aliases
                .contains(&("@/".to_string(), "app".to_string()))
        );
        assert!(
            result.entry_patterns.contains(
                &"app/custom/composables/**/*.{ts,tsx,js,jsx,mts,cts,mjs,cjs}".to_string()
            )
        );
        assert!(
            result
                .entry_patterns
                .contains(&"app/feature-components/**/*.{vue,ts,tsx,js,jsx}".to_string())
        );
        assert!(
            result
                .always_used_files
                .contains(&"app/app.config.{ts,js}".to_string())
        );
    }

    #[test]
    fn resolve_config_src_dir_overrides_default_app_aliases() {
        let source = r#"
            export default defineNuxtConfig({
                srcDir: "."
            });
        "#;
        let plugin = NuxtPlugin;
        let temp = tempfile::tempdir().expect("temp dir should be created");
        std::fs::create_dir(temp.path().join("app")).expect("app dir should exist");
        let config_path = temp.path().join("nuxt.config.ts");
        let result = plugin.resolve_config(&config_path, source, temp.path());

        assert!(
            result
                .path_aliases
                .contains(&("~/".to_string(), String::new())),
            "srcDir='.' should remap ~/ to the project root"
        );
        assert!(
            result
                .path_aliases
                .contains(&("@/".to_string(), String::new())),
            "srcDir='.' should remap @/ to the project root"
        );
    }

    #[test]
    fn resolve_config_src_dir_adds_custom_source_roots() {
        let source = r#"
            export default defineNuxtConfig({
                srcDir: "src/",
                imports: {
                    dirs: ["~/custom/composables"]
                },
                components: [
                    { path: "@/feature-components" }
                ]
            });
        "#;
        let plugin = NuxtPlugin;
        let result = plugin.resolve_config(
            Path::new("/project/nuxt.config.ts"),
            source,
            Path::new("/project"),
        );

        assert!(
            result
                .path_aliases
                .contains(&("~/".to_string(), "src".to_string())),
            "srcDir should remap ~/ to the configured source root"
        );
        assert!(
            result
                .path_aliases
                .contains(&("@/".to_string(), "src".to_string())),
            "srcDir should remap @/ to the configured source root"
        );
        assert!(
            result.entry_patterns.contains(
                &"src/custom/composables/**/*.{ts,tsx,js,jsx,mts,cts,mjs,cjs}".to_string()
            )
        );
        assert!(
            result
                .entry_patterns
                .contains(&"src/feature-components/**/*.{vue,ts,tsx,js,jsx}".to_string())
        );
        assert!(
            result
                .always_used_files
                .contains(&"src/app.vue".to_string()),
            "srcDir should add app.vue under the configured source root"
        );
        assert!(
            result
                .always_used_files
                .contains(&"src/app.config.{ts,js}".to_string()),
            "srcDir should add app.config under the configured source root"
        );
        assert!(
            result
                .always_used_files
                .contains(&"src/error.vue".to_string()),
            "srcDir should add error.vue under the configured source root"
        );
    }
}
