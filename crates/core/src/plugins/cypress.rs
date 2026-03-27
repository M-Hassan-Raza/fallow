//! Cypress test runner plugin.
//!
//! Detects Cypress projects and marks test files and support files as entry points.
//! Parses cypress.config to extract referenced dependencies.

use super::{Plugin, PluginResult};

const ENABLERS: &[&str] = &["cypress"];

const ENTRY_PATTERNS: &[&str] = &[
    "cypress/**/*.{ts,tsx,js,jsx}",
    "cypress/support/**/*.{ts,js}",
];

const CONFIG_PATTERNS: &[&str] = &["cypress.config.{ts,js,mjs,cjs}"];

const ALWAYS_USED: &[&str] = &["cypress.config.{ts,js,mjs,cjs}"];

const TOOLING_DEPENDENCIES: &[&str] = &["cypress", "@cypress/react", "@cypress/vue"];

define_plugin! {
    struct CypressPlugin => "cypress",
    enablers: ENABLERS,
    entry_patterns: ENTRY_PATTERNS,
    config_patterns: CONFIG_PATTERNS,
    always_used: ALWAYS_USED,
    tooling_dependencies: TOOLING_DEPENDENCIES,
    resolve_config: imports_only,
}
