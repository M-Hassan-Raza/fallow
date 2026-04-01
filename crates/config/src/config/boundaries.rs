//! Architecture boundary zone and rule definitions.

use globset::Glob;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Architecture boundary configuration.
///
/// Defines zones (directory groupings) and rules (which zones may import from which).
///
/// # Examples
///
/// ```
/// use fallow_config::BoundaryConfig;
///
/// let json = r#"{
///     "zones": [
///         { "name": "ui", "patterns": ["src/components/**"] },
///         { "name": "db", "patterns": ["src/db/**"] }
///     ],
///     "rules": [
///         { "from": "ui", "allow": ["db"] }
///     ]
/// }"#;
/// let config: BoundaryConfig = serde_json::from_str(json).unwrap();
/// assert_eq!(config.zones.len(), 2);
/// assert_eq!(config.rules.len(), 1);
/// ```
#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryConfig {
    /// Named zones mapping directory patterns to architectural layers.
    #[serde(default)]
    pub zones: Vec<BoundaryZone>,
    /// Import rules between zones. A zone with a rule entry can only import
    /// from the listed zones (plus itself). A zone without a rule entry is unrestricted.
    #[serde(default)]
    pub rules: Vec<BoundaryRule>,
}

/// A named zone grouping files by directory pattern.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryZone {
    /// Zone identifier referenced in rules (e.g., `"ui"`, `"database"`, `"shared"`).
    pub name: String,
    /// Glob patterns (relative to project root) that define zone membership.
    /// A file belongs to the first zone whose pattern matches.
    pub patterns: Vec<String>,
    /// Optional subtree scope. When set, patterns are relative to this directory
    /// instead of the project root. Useful for monorepos with per-package boundaries.
    /// Reserved for future use — currently ignored by the detector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

/// An import rule between zones.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryRule {
    /// The zone this rule applies to (the importing side).
    pub from: String,
    /// Zones that `from` is allowed to import from. Self-imports are always allowed.
    /// An empty list means the zone may not import from any other zone.
    #[serde(default)]
    pub allow: Vec<String>,
}

/// Resolved boundary config with pre-compiled glob matchers.
#[derive(Debug, Default)]
pub struct ResolvedBoundaryConfig {
    /// Zones with compiled glob matchers for fast file classification.
    pub zones: Vec<ResolvedZone>,
    /// Rules indexed by source zone name.
    pub rules: Vec<ResolvedBoundaryRule>,
}

/// A zone with pre-compiled glob matchers.
#[derive(Debug)]
pub struct ResolvedZone {
    /// Zone identifier.
    pub name: String,
    /// Pre-compiled glob matchers for zone membership.
    pub matchers: Vec<globset::GlobMatcher>,
}

/// A resolved boundary rule.
#[derive(Debug)]
pub struct ResolvedBoundaryRule {
    /// The zone this rule restricts.
    pub from_zone: String,
    /// Zones that `from_zone` is allowed to import from.
    pub allowed_zones: Vec<String>,
}

impl BoundaryConfig {
    /// Whether any boundaries are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Validate that all zone names referenced in rules are defined in `zones`.
    /// Returns a list of (rule_index, undefined_zone_name) pairs.
    #[must_use]
    pub fn validate_zone_references(&self) -> Vec<(usize, &str)> {
        let zone_names: rustc_hash::FxHashSet<&str> =
            self.zones.iter().map(|z| z.name.as_str()).collect();

        let mut errors = Vec::new();
        for (i, rule) in self.rules.iter().enumerate() {
            if !zone_names.contains(rule.from.as_str()) {
                errors.push((i, rule.from.as_str()));
            }
            for allowed in &rule.allow {
                if !zone_names.contains(allowed.as_str()) {
                    errors.push((i, allowed.as_str()));
                }
            }
        }
        errors
    }

    /// Resolve into compiled form with pre-built glob matchers.
    /// Invalid glob patterns are logged and skipped.
    #[must_use]
    pub fn resolve(&self) -> ResolvedBoundaryConfig {
        let zones = self
            .zones
            .iter()
            .map(|zone| {
                let matchers = zone
                    .patterns
                    .iter()
                    .filter_map(|pattern| match Glob::new(pattern) {
                        Ok(glob) => Some(glob.compile_matcher()),
                        Err(e) => {
                            tracing::warn!(
                                "invalid boundary zone glob pattern '{}' in zone '{}': {e}",
                                pattern,
                                zone.name
                            );
                            None
                        }
                    })
                    .collect();
                ResolvedZone {
                    name: zone.name.clone(),
                    matchers,
                }
            })
            .collect();

        let rules = self
            .rules
            .iter()
            .map(|rule| ResolvedBoundaryRule {
                from_zone: rule.from.clone(),
                allowed_zones: rule.allow.clone(),
            })
            .collect();

        ResolvedBoundaryConfig { zones, rules }
    }
}

impl ResolvedBoundaryConfig {
    /// Whether any boundaries are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Classify a file path into a zone. Returns the first matching zone name.
    /// Path should be relative to the project root with forward slashes.
    #[must_use]
    pub fn classify_zone(&self, relative_path: &str) -> Option<&str> {
        for zone in &self.zones {
            if zone.matchers.iter().any(|m| m.is_match(relative_path)) {
                return Some(&zone.name);
            }
        }
        None
    }

    /// Check if an import from `from_zone` to `to_zone` is allowed.
    /// Returns `true` if the import is permitted.
    #[must_use]
    pub fn is_import_allowed(&self, from_zone: &str, to_zone: &str) -> bool {
        // Self-imports are always allowed.
        if from_zone == to_zone {
            return true;
        }

        // Find the rule for the source zone.
        let rule = self.rules.iter().find(|r| r.from_zone == from_zone);

        match rule {
            // Zone has no rule entry — unrestricted.
            None => true,
            // Zone has a rule — check the allowlist.
            Some(r) => r.allowed_zones.iter().any(|z| z == to_zone),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config() {
        let config = BoundaryConfig::default();
        assert!(config.is_empty());
        assert!(config.validate_zone_references().is_empty());
    }

    #[test]
    fn deserialize_json() {
        let json = r#"{
            "zones": [
                { "name": "ui", "patterns": ["src/components/**", "src/pages/**"] },
                { "name": "db", "patterns": ["src/db/**"] },
                { "name": "shared", "patterns": ["src/shared/**"] }
            ],
            "rules": [
                { "from": "ui", "allow": ["shared"] },
                { "from": "db", "allow": ["shared"] }
            ]
        }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.zones.len(), 3);
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.zones[0].name, "ui");
        assert_eq!(
            config.zones[0].patterns,
            vec!["src/components/**", "src/pages/**"]
        );
        assert_eq!(config.rules[0].from, "ui");
        assert_eq!(config.rules[0].allow, vec!["shared"]);
    }

    #[test]
    fn deserialize_toml() {
        let toml_str = r#"
[[zones]]
name = "ui"
patterns = ["src/components/**"]

[[zones]]
name = "db"
patterns = ["src/db/**"]

[[rules]]
from = "ui"
allow = ["db"]
"#;
        let config: BoundaryConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.zones.len(), 2);
        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn validate_zone_references_valid() {
        let config = BoundaryConfig {
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["db".to_string()],
            }],
        };
        assert!(config.validate_zone_references().is_empty());
    }

    #[test]
    fn validate_zone_references_invalid_from() {
        let config = BoundaryConfig {
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "databse".to_string(),
                allow: vec!["ui".to_string()],
            }],
        };
        let errors = config.validate_zone_references();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1, "databse");
    }

    #[test]
    fn validate_zone_references_invalid_allow() {
        let config = BoundaryConfig {
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["nonexistent".to_string()],
            }],
        };
        let errors = config.validate_zone_references();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1, "nonexistent");
    }

    #[test]
    fn resolve_and_classify() {
        let config = BoundaryConfig {
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec!["src/components/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec!["src/db/**".to_string()],
                    root: None,
                },
            ],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert_eq!(
            resolved.classify_zone("src/components/Button.tsx"),
            Some("ui")
        );
        assert_eq!(resolved.classify_zone("src/db/queries.ts"), Some("db"));
        assert_eq!(resolved.classify_zone("src/utils/helpers.ts"), None);
    }

    #[test]
    fn first_match_wins() {
        let config = BoundaryConfig {
            zones: vec![
                BoundaryZone {
                    name: "specific".to_string(),
                    patterns: vec!["src/shared/db-utils/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec!["src/shared/**".to_string()],
                    root: None,
                },
            ],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert_eq!(
            resolved.classify_zone("src/shared/db-utils/pool.ts"),
            Some("specific")
        );
        assert_eq!(
            resolved.classify_zone("src/shared/helpers.ts"),
            Some("shared")
        );
    }

    #[test]
    fn self_import_always_allowed() {
        let config = BoundaryConfig {
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec![],
            }],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("ui", "ui"));
    }

    #[test]
    fn unrestricted_zone_allows_all() {
        let config = BoundaryConfig {
            zones: vec![
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("shared", "db"));
    }

    #[test]
    fn restricted_zone_blocks_unlisted() {
        let config = BoundaryConfig {
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["shared".to_string()],
            }],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("ui", "shared"));
        assert!(!resolved.is_import_allowed("ui", "db"));
    }

    #[test]
    fn empty_allow_blocks_all_except_self() {
        let config = BoundaryConfig {
            zones: vec![
                BoundaryZone {
                    name: "isolated".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "other".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "isolated".to_string(),
                allow: vec![],
            }],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("isolated", "isolated"));
        assert!(!resolved.is_import_allowed("isolated", "other"));
    }

    #[test]
    fn root_field_reserved() {
        let json = r#"{
            "zones": [{ "name": "ui", "patterns": ["src/**"], "root": "packages/app/" }],
            "rules": []
        }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.zones[0].root.as_deref(), Some("packages/app/"));
    }
}
