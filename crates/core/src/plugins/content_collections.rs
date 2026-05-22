//! Content Collections plugin.
//!
//! Detects Content Collections projects and marks the root config as used.

use super::Plugin;

const ENABLERS: &[&str] = &["@content-collections/core"];

const ENTRY_PATTERNS: &[&str] = &["content-collections.{ts,tsx,js,jsx,mts,mjs,cts,cjs}"];

const TOOLING_DEPENDENCIES: &[&str] = &[
    "@content-collections/core",
    "@content-collections/vite",
    "@content-collections/next",
    "@content-collections/remix-vite",
    "@content-collections/vinxi",
    "@content-collections/markdown",
    "@content-collections/mdx",
];

define_plugin! {
    struct ContentCollectionsPlugin => "content-collections",
    enablers: ENABLERS,
    entry_patterns: ENTRY_PATTERNS,
    tooling_dependencies: TOOLING_DEPENDENCIES,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_root_config_and_tooling_packages() {
        let plugin = ContentCollectionsPlugin;

        assert!(
            plugin
                .entry_patterns()
                .contains(&"content-collections.{ts,tsx,js,jsx,mts,mjs,cts,cjs}"),
            "entry pattern must accept every JS/TS extension content-collections honors at runtime"
        );
        assert!(
            plugin
                .tooling_dependencies()
                .contains(&"@content-collections/vite")
        );
    }
}
