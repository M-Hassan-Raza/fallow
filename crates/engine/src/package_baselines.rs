//! Per-workspace Git baselines for changed-file result scoping.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fallow_config::WorkspaceInfo;
use rustc_hash::FxHashSet;

use crate::changed_files::{ChangedFilesError, ChangedPathScope};

/// Failure to resolve an authored workspace baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageBaselineError {
    /// The project or a discovered workspace root could not be resolved.
    UnavailableRoot {
        /// Root that could not be resolved.
        path: PathBuf,
        /// Filesystem error detail.
        message: String,
    },
    /// A key is not an exact, relative, slash-separated workspace root.
    InvalidWorkspaceKey {
        /// Authored key.
        key: String,
    },
    /// No discovered workspace has this root.
    UnknownWorkspace {
        /// Authored key.
        key: String,
    },
    /// Two authored keys resolve to the same discovered workspace.
    DuplicateWorkspace {
        /// Earlier authored key.
        first: String,
        /// Later authored key.
        second: String,
    },
    /// Git could not resolve a package's baseline ref.
    Git {
        /// Authored workspace key.
        key: String,
        /// Authored Git ref.
        reference: String,
        /// Underlying Git error.
        source: ChangedFilesError,
    },
}

impl fmt::Display for PackageBaselineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnavailableRoot { path, message } => {
                write!(
                    f,
                    "cannot resolve workspace root '{}': {message}",
                    path.display()
                )
            }
            Self::InvalidWorkspaceKey { key } => write!(
                f,
                "workspace baseline key '{key}' must be an exact project-relative workspace root"
            ),
            Self::UnknownWorkspace { key } => {
                write!(
                    f,
                    "workspace baseline key '{key}' names no discovered workspace"
                )
            }
            Self::DuplicateWorkspace { first, second } => write!(
                f,
                "workspace baseline keys '{first}' and '{second}' name the same workspace"
            ),
            Self::Git {
                key,
                reference,
                source,
            } => write!(
                f,
                "workspace baseline '{reference}' for '{key}' failed: {}",
                source.describe()
            ),
        }
    }
}

impl std::error::Error for PackageBaselineError {}

#[derive(Debug, Clone)]
enum WorkspaceBaseline {
    Full,
    Changed {
        reference: String,
        files: Arc<FxHashSet<PathBuf>>,
    },
}

/// Resolved package baselines for one analysis project.
///
/// A file belongs to its nearest discovered workspace root. Workspaces with
/// no authored baseline and files outside workspace roots remain in full scope.
#[derive(Debug, Clone)]
pub struct PackageChangeScope {
    root: PathBuf,
    authored_root: PathBuf,
    workspaces: BTreeMap<PathBuf, WorkspaceBaseline>,
}

impl PackageChangeScope {
    /// Resolve configured refs and discovered workspace ownership atomically.
    /// An empty map means no package scope was requested.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid workspace key, an undiscovered package,
    /// an unavailable root, or any Git ref that cannot be resolved.
    pub fn resolve(
        root: &Path,
        configured: &BTreeMap<String, String>,
        workspaces: &[WorkspaceInfo],
    ) -> Result<Option<Self>, PackageBaselineError> {
        if configured.is_empty() {
            return Ok(None);
        }

        let authored_root = dunce::simplified(root).to_path_buf();
        let root = canonical_root(root)?;
        let mut packages = BTreeMap::new();
        for workspace in workspaces {
            packages.insert(canonical_root(&workspace.root)?, WorkspaceBaseline::Full);
        }

        let mut assigned = BTreeMap::<PathBuf, String>::new();
        let mut validated = Vec::with_capacity(configured.len());
        for (key, reference) in configured {
            if !valid_workspace_key(key) {
                return Err(PackageBaselineError::InvalidWorkspaceKey { key: key.clone() });
            }
            let candidate = root.join(key);
            if !candidate.exists() {
                return Err(PackageBaselineError::UnknownWorkspace { key: key.clone() });
            }
            let path = canonical_root(&candidate)?;
            if !path.starts_with(&root) || !packages.contains_key(&path) {
                return Err(PackageBaselineError::UnknownWorkspace { key: key.clone() });
            }
            if let Some(first) = assigned.insert(path.clone(), key.clone()) {
                return Err(PackageBaselineError::DuplicateWorkspace {
                    first,
                    second: key.clone(),
                });
            }
            validated.push((key, reference, path));
        }

        let mut refs = BTreeMap::<String, Arc<FxHashSet<PathBuf>>>::new();
        for (key, reference, path) in validated {
            let files = if let Some(files) = refs.get(reference) {
                Arc::clone(files)
            } else {
                let files =
                    crate::changed_files::changed_files(&root, reference).map_err(|source| {
                        PackageBaselineError::Git {
                            key: key.clone(),
                            reference: reference.clone(),
                            source,
                        }
                    })?;
                let files = Arc::new(
                    files
                        .into_iter()
                        .map(|path| dunce::simplified(&path).to_path_buf())
                        .collect(),
                );
                refs.insert(reference.clone(), Arc::clone(&files));
                files
            };
            packages.insert(
                path,
                WorkspaceBaseline::Changed {
                    reference: reference.clone(),
                    files,
                },
            );
        }

        Ok(Some(Self {
            root,
            authored_root,
            workspaces: packages,
        }))
    }

    /// Effective Git ref for a file, or `None` in a full-scope workspace.
    #[must_use]
    pub fn baseline_for(&self, path: &Path) -> Option<&str> {
        match self.owner_absolute(&self.absolute_path(path)) {
            Some(WorkspaceBaseline::Changed { reference, .. }) => Some(reference),
            Some(WorkspaceBaseline::Full) | None => None,
        }
    }

    /// Whether a finding owner path belongs to this mixed scope.
    #[must_use]
    pub fn includes(&self, path: &Path) -> bool {
        let absolute = self.absolute_path(path);
        match self.owner_absolute(&absolute) {
            Some(WorkspaceBaseline::Changed { files, .. }) => files.contains(&absolute),
            Some(WorkspaceBaseline::Full) | None => true,
        }
    }

    fn owner_absolute(&self, path: &Path) -> Option<&WorkspaceBaseline> {
        path.ancestors()
            .find_map(|ancestor| self.workspaces.get(ancestor))
    }

    fn absolute_path(&self, path: &Path) -> PathBuf {
        let absolute = if path.is_absolute() {
            if let Ok(relative) = path.strip_prefix(&self.authored_root) {
                self.root.join(relative)
            } else {
                path.to_path_buf()
            }
        } else {
            self.root.join(path)
        };
        dunce::simplified(&absolute).to_path_buf()
    }
}

impl ChangedPathScope for PackageChangeScope {
    fn contains(&self, path: &Path) -> bool {
        self.includes(path)
    }
}

fn canonical_root(path: &Path) -> Result<PathBuf, PackageBaselineError> {
    dunce::canonicalize(path).map_err(|err| PackageBaselineError::UnavailableRoot {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

fn valid_workspace_key(key: &str) -> bool {
    !key.is_empty()
        && !key.contains('\\')
        && key
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && !Path::new(key).is_absolute()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    use fallow_types::output_dead_code::{UnresolvedCatalogReferenceFinding, UnusedFileFinding};
    use fallow_types::results::{AnalysisResults, UnresolvedCatalogReference, UnusedFile};

    fn git(root: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        crate::changed_files::clear_ambient_git_env(&mut command);
        let output = command
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn workspace(root: &Path, relative: &str) -> WorkspaceInfo {
        WorkspaceInfo {
            root: root.join(relative),
            name: relative.to_owned(),
            is_internal_dependency: false,
        }
    }

    fn nested_repo() -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let parent = "packages/parent";
        let child = "packages/parent/packages/child";
        let other = "packages/other";
        for package in [parent, child, other] {
            fs::create_dir_all(root.join(package)).expect("package directory");
            fs::write(
                root.join(package).join("index.ts"),
                "export const value = 1;",
            )
            .expect("source");
            fs::write(root.join(package).join("package.json"), "{}").expect("manifest");
        }
        git(root, &["init", "-q"]);
        git(root, &["add", "."]);
        git(
            root,
            &[
                "-c",
                "user.name=Fallow Test",
                "-c",
                "user.email=fallow@example.test",
                "commit",
                "-qm",
                "base",
            ],
        );
        git(root, &["branch", "base"]);
        fs::write(root.join(child).join("index.ts"), "export const value = 2;")
            .expect("child change");
        git(root, &["add", "."]);
        git(
            root,
            &[
                "-c",
                "user.name=Fallow Test",
                "-c",
                "user.email=fallow@example.test",
                "commit",
                "-qm",
                "child change",
            ],
        );
        fs::write(
            root.join(parent).join("index.ts"),
            "export const value = 3;",
        )
        .expect("parent change");

        temp
    }

    #[test]
    fn nested_refs_scope_source_and_manifest_owners() {
        let temp = nested_repo();
        let root = temp.path();
        let parent = "packages/parent";
        let child = "packages/parent/packages/child";
        let other = "packages/other";
        let workspaces = [
            workspace(root, parent),
            workspace(root, child),
            workspace(root, other),
        ];
        let configured = BTreeMap::from([
            (parent.to_owned(), "base".to_owned()),
            (child.to_owned(), "HEAD".to_owned()),
        ]);
        let scope = PackageChangeScope::resolve(root, &configured, &workspaces)
            .expect("valid refs")
            .expect("package scope");
        assert!(scope.includes(&root.join(parent).join("index.ts")));
        assert!(!scope.includes(&root.join(child).join("index.ts")));
        assert!(scope.includes(&root.join(other).join("package.json")));
        assert!(scope.includes(&root.join("root.ts")));
        assert_eq!(
            scope.baseline_for(&root.join(child).join("index.ts")),
            Some("HEAD")
        );
        assert_eq!(scope.baseline_for(&root.join(other).join("index.ts")), None);

        let mut results = AnalysisResults::default();
        for package in [parent, child] {
            results
                .unused_files
                .push(UnusedFileFinding::with_actions(UnusedFile {
                    path: root.join(package).join("index.ts"),
                }));
        }
        results.unresolved_catalog_references.push(
            UnresolvedCatalogReferenceFinding::with_actions(UnresolvedCatalogReference {
                entry_name: "react".to_owned(),
                catalog_name: "default".to_owned(),
                path: root.join(other).join("package.json"),
                line: 1,
                available_in_catalogs: Vec::new(),
            }),
        );
        crate::changed_files::filter_results_by_path_scope(&mut results, &scope);
        assert_eq!(results.unused_files.len(), 1);
        assert_eq!(
            results.unused_files[0].file.path,
            root.join(parent).join("index.ts")
        );
        assert_eq!(results.unresolved_catalog_references.len(), 1);
    }

    #[test]
    fn invalid_ref_and_clean_packages_have_explicit_scope() {
        let temp = nested_repo();
        let root = temp.path();
        let parent = "packages/parent";
        let child = "packages/parent/packages/child";
        let other = "packages/other";
        let workspaces = [
            workspace(root, parent),
            workspace(root, child),
            workspace(root, other),
        ];
        let invalid_ref = BTreeMap::from([(parent.to_owned(), "missing-ref".to_owned())]);
        assert!(matches!(
            PackageChangeScope::resolve(root, &invalid_ref, &workspaces),
            Err(PackageBaselineError::Git { .. })
        ));

        git(root, &["add", "."]);
        git(
            root,
            &[
                "-c",
                "user.name=Fallow Test",
                "-c",
                "user.email=fallow@example.test",
                "commit",
                "-qm",
                "parent change",
            ],
        );
        let all_mapped = BTreeMap::from([
            (parent.to_owned(), "HEAD".to_owned()),
            (child.to_owned(), "HEAD".to_owned()),
            (other.to_owned(), "HEAD".to_owned()),
        ]);
        let clean = PackageChangeScope::resolve(root, &all_mapped, &workspaces)
            .expect("valid HEAD")
            .expect("package scope");
        assert!(!clean.includes(&root.join(parent).join("index.ts")));
        assert!(!clean.includes(&root.join(child).join("index.ts")));
        assert!(!clean.includes(&root.join(other).join("package.json")));
        assert!(clean.includes(&root.join("root.ts")));
    }

    #[test]
    fn invalid_mapping_fails_before_scope_is_applied() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir_all(root.join("packages/app")).expect("package directory");
        let workspaces = [workspace(root, "packages/app")];
        let invalid_key = BTreeMap::from([("packages/../app".to_owned(), "HEAD".to_owned())]);
        assert!(matches!(
            PackageChangeScope::resolve(root, &invalid_key, &workspaces),
            Err(PackageBaselineError::InvalidWorkspaceKey { .. })
        ));
        let unknown = BTreeMap::from([("packages/missing".to_owned(), "HEAD".to_owned())]);
        assert!(matches!(
            PackageChangeScope::resolve(root, &unknown, &workspaces),
            Err(PackageBaselineError::UnknownWorkspace { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_keys_cannot_assign_one_workspace_twice() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir_all(root.join("packages/app")).expect("package directory");
        std::os::unix::fs::symlink(root.join("packages/app"), root.join("alias"))
            .expect("workspace alias");
        let configured = BTreeMap::from([
            ("alias".to_owned(), "HEAD".to_owned()),
            ("packages/app".to_owned(), "HEAD".to_owned()),
        ]);
        assert!(matches!(
            PackageChangeScope::resolve(root, &configured, &[workspace(root, "packages/app")]),
            Err(PackageBaselineError::DuplicateWorkspace { .. })
        ));
    }
}
