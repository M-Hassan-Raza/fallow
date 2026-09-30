//! The Git change scope of one analysis run.
//!
//! A run narrows its findings by at most one change scope: the changed files
//! of one global ref, or the per-workspace refs of `workspaces.changedSince`.
//! Every surface describes its inputs with a [`ChangeScopeRequest`] and calls
//! [`ChangeScope::resolve`], which owns the precedence rule. The resolved
//! value owns the three things that must agree: the result filter, the
//! `package_baselines` provenance rows, and the scope flag that baseline
//! comparison and finding-id queries read. A surface therefore cannot apply
//! the filter and forget the flag, or read the package map where the caller
//! owns the scope.
//!
//! The filter applies to the final result of the run. A surface that narrows
//! findings before type-aware refinement, to reduce sidecar work, applies the
//! same scope again after refinement, because refinement can add findings.

use std::path::{Path, PathBuf};

use fallow_config::{ResolvedConfig, WorkspaceInfo};
use fallow_output::PackageBaselineStatus;
use fallow_types::duplicates::DuplicationReport;
use fallow_types::results::AnalysisResults;
use rustc_hash::FxHashSet;

use crate::package_baselines::{PackageBaselineError, PackageChangeScope};

/// Who owns the change scope of a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChangeScopeOwner {
    /// The run owns it: a global ref when one is requested, otherwise the
    /// configured package baselines.
    #[default]
    Run,
    /// The calling pipeline narrows the findings itself. `audit` is the
    /// example: it compares a head run and a base run against its own changed
    /// files. The run reads no package baselines, so it never resolves Git
    /// refs in a base snapshot and never hides a finding that the comparison
    /// needs.
    Caller,
}

/// The change-scope inputs of one run.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChangeScopeRequest<'a> {
    /// Who owns the scope.
    pub owner: ChangeScopeOwner,
    /// A global changed-since ref was requested. This is `true` also when the
    /// ref did not resolve: the run then reports in full scope, and the
    /// package baselines still do not apply.
    pub global_ref: bool,
    /// The changed files of the global ref, or a changed-file set that the
    /// caller supplied.
    pub files: Option<&'a FxHashSet<PathBuf>>,
}

impl ChangeScopeRequest<'_> {
    /// Whether [`ChangeScope::resolve`] reads `workspaces.changedSince` for
    /// this request. A surface that discovers workspaces only for the package
    /// map checks this first.
    #[must_use]
    pub fn reads_package_baselines(&self, config: &ResolvedConfig) -> bool {
        self.owner == ChangeScopeOwner::Run
            && !self.global_ref
            && self.files.is_none()
            && !config.workspace_changed_since.is_empty()
    }
}

/// The resolved change scope of one run.
#[derive(Debug, Clone, Default)]
pub struct ChangeScope<'a> {
    kind: ChangeScopeKind<'a>,
    global_ref: bool,
}

#[derive(Debug, Clone, Default)]
enum ChangeScopeKind<'a> {
    #[default]
    Full,
    Files(&'a FxHashSet<PathBuf>),
    Packages(PackageChangeScope),
}

impl<'a> ChangeScope<'a> {
    /// Resolve the change scope of a run.
    ///
    /// A changed-file set wins. A requested global ref without files, or a
    /// caller-owned scope, gives the full scope. Otherwise the configured
    /// package baselines apply, when the config has any.
    ///
    /// # Errors
    ///
    /// Returns an error when the package map names an invalid or unknown
    /// workspace root, or a Git ref that does not resolve.
    pub fn resolve(
        request: ChangeScopeRequest<'a>,
        config: &ResolvedConfig,
        workspaces: &[WorkspaceInfo],
    ) -> Result<Self, PackageBaselineError> {
        let global_ref = request.global_ref || request.files.is_some();
        if let Some(files) = request.files {
            return Ok(Self {
                kind: ChangeScopeKind::Files(files),
                global_ref,
            });
        }
        if !request.reads_package_baselines(config) {
            return Ok(Self {
                kind: ChangeScopeKind::Full,
                global_ref,
            });
        }
        let kind =
            PackageChangeScope::resolve(&config.root, &config.workspace_changed_since, workspaces)?
                .map_or(ChangeScopeKind::Full, ChangeScopeKind::Packages);
        Ok(Self { kind, global_ref })
    }

    /// The scope of one global changed-file set.
    #[must_use]
    pub const fn changed_files(files: &'a FxHashSet<PathBuf>) -> Self {
        Self {
            kind: ChangeScopeKind::Files(files),
            global_ref: true,
        }
    }

    /// Whether a change ref narrows the run. Baseline comparison and
    /// finding-id queries read this: a finding outside the scope is hidden,
    /// not gone.
    #[must_use]
    pub const fn is_change_scoped(&self) -> bool {
        self.global_ref || matches!(self.kind, ChangeScopeKind::Packages(_))
    }

    /// The applied package baselines, when the configured map scopes the run.
    #[must_use]
    pub const fn packages(&self) -> Option<&PackageChangeScope> {
        match &self.kind {
            ChangeScopeKind::Packages(packages) => Some(packages),
            ChangeScopeKind::Full | ChangeScopeKind::Files(_) => None,
        }
    }

    /// The `package_baselines` provenance rows of the run. Empty unless the
    /// configured map scopes the run.
    #[must_use]
    pub fn package_baselines(&self) -> Vec<PackageBaselineStatus> {
        self.packages().map_or_else(Vec::new, |packages| {
            package_baseline_statuses(std::slice::from_ref(packages), packages.project_root())
        })
    }

    /// Whether a finding owned by `path` is in scope.
    #[must_use]
    pub fn contains(&self, path: &Path) -> bool {
        match &self.kind {
            ChangeScopeKind::Full => true,
            ChangeScopeKind::Files(files) => {
                files.contains(path) || files.contains(dunce::simplified(path))
            }
            ChangeScopeKind::Packages(packages) => packages.includes(path),
        }
    }

    /// Keep the dead-code findings in scope.
    pub(crate) fn retain_dead_code(&self, results: &mut AnalysisResults) {
        match &self.kind {
            ChangeScopeKind::Full => {}
            ChangeScopeKind::Files(files) => {
                crate::changed_files::filter_results_by_changed_files(results, files);
            }
            ChangeScopeKind::Packages(packages) => {
                crate::changed_files::filter_results_by_path_scope(results, packages);
            }
        }
    }

    /// Keep the clone groups with at least one instance in scope.
    pub(crate) fn retain_duplication(&self, report: &mut DuplicationReport, root: &Path) {
        match &self.kind {
            ChangeScopeKind::Full => {}
            ChangeScopeKind::Files(files) => {
                crate::changed_files::filter_duplication_by_changed_files(report, files, root);
            }
            ChangeScopeKind::Packages(packages) => {
                crate::changed_files::filter_duplication_by_package_scope(report, packages, root);
            }
        }
    }
}

/// Project-relative package baseline rows, sorted by workspace root.
///
/// An editor that analyzes several project roots passes every applied scope
/// and its own root, so the rows of all projects share one path base.
#[must_use]
pub fn package_baseline_statuses(
    scopes: &[PackageChangeScope],
    root: &Path,
) -> Vec<PackageBaselineStatus> {
    let root = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut rows = scopes
        .iter()
        .flat_map(PackageChangeScope::configured_baselines)
        .map(|(path, reference)| PackageBaselineStatus {
            workspace_root: path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/"),
            reference: reference.to_owned(),
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.workspace_root.cmp(&b.workspace_root));
    rows
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fallow_config::{FallowConfig, OutputFormat};

    use super::*;

    fn config_with_map(root: &Path, map: &[(&str, &str)]) -> ResolvedConfig {
        let mut config = FallowConfig::default().resolve(
            root.to_path_buf(),
            OutputFormat::Json,
            1,
            true,
            true,
            None,
        );
        config.workspace_changed_since = map
            .iter()
            .map(|(key, reference)| ((*key).to_owned(), (*reference).to_owned()))
            .collect::<BTreeMap<_, _>>();
        config
    }

    /// A map that names no discovered workspace fails resolution. The tests
    /// below use it to prove that a request never reads the map.
    fn unknown_workspace_map(root: &Path) -> ResolvedConfig {
        config_with_map(root, &[("packages/missing", "HEAD")])
    }

    #[test]
    fn caller_owned_scope_never_reads_the_package_map() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = unknown_workspace_map(temp.path());
        let request = ChangeScopeRequest {
            owner: ChangeScopeOwner::Caller,
            ..ChangeScopeRequest::default()
        };
        assert!(!request.reads_package_baselines(&config));
        let scope = ChangeScope::resolve(request, &config, &[]).expect("caller-owned scope");
        assert!(!scope.is_change_scoped());
        assert!(scope.package_baselines().is_empty());
        assert!(scope.contains(&temp.path().join("packages/a/index.ts")));
    }

    #[test]
    fn a_requested_global_ref_suppresses_the_map_even_without_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = unknown_workspace_map(temp.path());
        let request = ChangeScopeRequest {
            global_ref: true,
            ..ChangeScopeRequest::default()
        };
        let scope = ChangeScope::resolve(request, &config, &[]).expect("global scope");
        assert!(scope.is_change_scoped());
        assert!(scope.packages().is_none());
    }

    #[test]
    fn a_changed_file_set_wins_over_the_map() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = unknown_workspace_map(temp.path());
        let changed: FxHashSet<PathBuf> = std::iter::once(temp.path().join("a.ts")).collect();
        let request = ChangeScopeRequest {
            files: Some(&changed),
            ..ChangeScopeRequest::default()
        };
        let scope = ChangeScope::resolve(request, &config, &[]).expect("file scope");
        assert!(scope.is_change_scoped());
        assert!(scope.contains(&temp.path().join("a.ts")));
        assert!(!scope.contains(&temp.path().join("b.ts")));
    }

    #[test]
    fn a_run_owned_scope_reads_the_map() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = unknown_workspace_map(temp.path());
        assert!(matches!(
            ChangeScope::resolve(ChangeScopeRequest::default(), &config, &[]),
            Err(PackageBaselineError::UnknownWorkspace { .. })
        ));
        let empty = config_with_map(temp.path(), &[]);
        let scope = ChangeScope::resolve(ChangeScopeRequest::default(), &empty, &[])
            .expect("no map, full scope");
        assert!(!scope.is_change_scoped());
    }
}
