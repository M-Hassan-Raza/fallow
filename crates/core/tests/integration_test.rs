#[path = "integration_test/common.rs"]
mod common;

#[path = "integration_test/barrel_exports.rs"]
mod barrel_exports;
#[path = "integration_test/basic_analysis.rs"]
mod basic_analysis;
#[path = "integration_test/caching.rs"]
mod caching;
#[path = "integration_test/css_modules.rs"]
mod css_modules;
#[path = "integration_test/dependencies.rs"]
mod dependencies;
#[path = "integration_test/duplicates.rs"]
mod duplicates;
#[path = "integration_test/dynamic_imports.rs"]
mod dynamic_imports;
#[path = "integration_test/external_plugins.rs"]
mod external_plugins;
#[path = "integration_test/extraction.rs"]
mod extraction;
#[path = "integration_test/frameworks.rs"]
mod frameworks;
#[path = "integration_test/member_detection.rs"]
mod member_detection;
#[path = "integration_test/rules_config.rs"]
mod rules_config;
#[path = "integration_test/sfc_parsing.rs"]
mod sfc_parsing;
#[path = "integration_test/unreachable_exports.rs"]
mod unreachable_exports;
#[path = "integration_test/workspaces.rs"]
mod workspaces;
