//! React Native and Expo platform extension support.

use super::types::RN_PLATFORM_PREFIXES;

/// Check if React Native or Expo plugins are active.
pub(super) fn has_react_native_plugin(active_plugins: &[String]) -> bool {
    active_plugins
        .iter()
        .any(|p| p == "react-native" || p == "expo")
}

/// Build the resolver extension list, optionally prepending React Native platform
/// extensions when the RN/Expo plugin is active.
pub(super) fn build_extensions(active_plugins: &[String]) -> Vec<String> {
    // Declaration files (.d.ts, .d.mts, .d.cts) must come AFTER runtime files
    // (.js, .jsx, etc.). When both exist side-by-side (e.g. suspense.js +
    // suspense.d.ts), the import should resolve to the runtime module, not the
    // type declaration. Declarations provide types for their companion .js files
    // but are not standalone modules.
    let base: Vec<String> = vec![
        ".ts".into(),
        ".tsx".into(),
        ".mts".into(),
        ".cts".into(),
        ".js".into(),
        ".jsx".into(),
        ".mjs".into(),
        ".cjs".into(),
        ".d.ts".into(),
        ".d.mts".into(),
        ".d.cts".into(),
        ".json".into(),
        ".vue".into(),
        ".svelte".into(),
        ".astro".into(),
        ".mdx".into(),
        ".css".into(),
        ".scss".into(),
    ];

    if has_react_native_plugin(active_plugins) {
        let source_exts = [".ts", ".tsx", ".js", ".jsx"];
        let mut rn_extensions: Vec<String> = Vec::new();
        for platform in RN_PLATFORM_PREFIXES {
            for ext in &source_exts {
                rn_extensions.push(format!("{platform}{ext}"));
            }
        }
        rn_extensions.extend(base);
        rn_extensions
    } else {
        base
    }
}

/// Build the resolver `condition_names` list, optionally prepending React Native
/// conditions when the RN/Expo plugin is active.
pub(super) fn build_condition_names(active_plugins: &[String]) -> Vec<String> {
    let mut names = vec![
        "import".into(),
        "require".into(),
        "default".into(),
        "types".into(),
        "node".into(),
    ];
    if has_react_native_plugin(active_plugins) {
        names.insert(0, "react-native".into());
        names.insert(1, "browser".into());
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_react_native_plugin_active() {
        let plugins = vec!["react-native".to_string(), "typescript".to_string()];
        assert!(has_react_native_plugin(&plugins));
    }

    #[test]
    fn test_has_expo_plugin_active() {
        let plugins = vec!["expo".to_string(), "typescript".to_string()];
        assert!(has_react_native_plugin(&plugins));
    }

    #[test]
    fn test_has_react_native_plugin_inactive() {
        let plugins = vec!["nextjs".to_string(), "typescript".to_string()];
        assert!(!has_react_native_plugin(&plugins));
    }

    #[test]
    fn test_rn_platform_extensions_prepended() {
        let no_rn = build_extensions(&[]);
        let rn_plugins = vec!["react-native".to_string()];
        let with_rn = build_extensions(&rn_plugins);

        // Without RN, the first extension should be .ts
        assert_eq!(no_rn[0], ".ts");

        // With RN, platform extensions should come first
        assert_eq!(with_rn[0], ".web.ts");
        assert_eq!(with_rn[1], ".web.tsx");
        assert_eq!(with_rn[2], ".web.js");
        assert_eq!(with_rn[3], ".web.jsx");

        // Verify all 4 platforms (web, ios, android, native) x 4 exts = 16
        assert!(with_rn.len() > no_rn.len());
        assert_eq!(
            with_rn.len(),
            no_rn.len() + 16,
            "should add 16 platform extensions (4 platforms x 4 exts)"
        );
    }

    #[test]
    fn test_rn_condition_names_prepended() {
        let no_rn = build_condition_names(&[]);
        let rn_plugins = vec!["react-native".to_string()];
        let with_rn = build_condition_names(&rn_plugins);

        // Without RN, first condition should be "import"
        assert_eq!(no_rn[0], "import");

        // With RN, "react-native" and "browser" should be prepended
        assert_eq!(with_rn[0], "react-native");
        assert_eq!(with_rn[1], "browser");
        assert_eq!(with_rn[2], "import");
    }
}
