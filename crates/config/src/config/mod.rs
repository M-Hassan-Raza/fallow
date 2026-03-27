mod duplicates_config;
mod format;
mod health;
mod parsing;
mod resolution;
mod rules;

pub use duplicates_config::{
    DetectionMode, DuplicatesConfig, NormalizationConfig, ResolvedNormalization,
};
pub use format::OutputFormat;
pub use health::HealthConfig;
pub use resolution::{ConfigOverride, IgnoreExportRule, ResolvedConfig, ResolvedOverride};
pub use rules::{PartialRulesConfig, RulesConfig, Severity};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::external_plugin::ExternalPluginDef;
use crate::workspace::WorkspaceConfig;

/// User-facing configuration loaded from `.fallowrc.json` or `fallow.toml`.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FallowConfig {
    /// JSON Schema reference (ignored during deserialization).
    #[serde(rename = "$schema", default, skip_serializing)]
    #[schemars(skip)]
    pub schema: Option<String>,

    /// Paths to base config files to extend from.
    /// Paths are resolved relative to the config file containing the `extends`.
    /// Base configs are loaded first, then this config's values override them.
    /// Later entries in the array override earlier ones.
    #[serde(default, skip_serializing)]
    pub extends: Vec<String>,

    /// Additional entry point glob patterns.
    #[serde(default)]
    pub entry: Vec<String>,

    /// Glob patterns to ignore from analysis.
    #[serde(default)]
    pub ignore_patterns: Vec<String>,

    /// Custom framework definitions (inline plugin definitions).
    #[serde(default)]
    pub framework: Vec<ExternalPluginDef>,

    /// Workspace overrides.
    #[serde(default)]
    pub workspaces: Option<WorkspaceConfig>,

    /// Dependencies to ignore (always considered used).
    #[serde(default)]
    pub ignore_dependencies: Vec<String>,

    /// Export ignore rules.
    #[serde(default)]
    pub ignore_exports: Vec<IgnoreExportRule>,

    /// Duplication detection settings.
    #[serde(default)]
    pub duplicates: DuplicatesConfig,

    /// Complexity health metrics settings.
    #[serde(default)]
    pub health: HealthConfig,

    /// Per-issue-type severity rules.
    #[serde(default)]
    pub rules: RulesConfig,

    /// Production mode: exclude test/dev files, only start/build scripts.
    #[serde(default)]
    pub production: bool,

    /// Paths to external plugin files or directories containing plugin files.
    ///
    /// Supports TOML, JSON, and JSONC formats.
    ///
    /// In addition to these explicit paths, fallow automatically discovers:
    /// - `*.toml`, `*.json`, `*.jsonc` files in `.fallow/plugins/`
    /// - `fallow-plugin-*.{toml,json,jsonc}` files in the project root
    #[serde(default)]
    pub plugins: Vec<String>,

    /// Per-file rule overrides matching oxlint's overrides pattern.
    #[serde(default)]
    pub overrides: Vec<ConfigOverride>,
}
