//! Cross-package propagation for namespace-import object aliases.
//!
//! When a barrel re-exports a namespace import inside an object literal
//! (`import * as foo from './bar'; export const API = { foo }`), a downstream
//! consumer accessing `API.foo.bar` would lose the connection between `bar`
//! and the namespace target file because `narrow_namespace_references` only
//! scans member accesses in the file that contains the `import *`. This
//! module propagates each consumer's `<imported>.<suffix>.<member>` access
//! onto the namespace target's matching export so cross-package access does
//! not surface as a false `unused-export`. See issue #303.
//!
//! Runs once after Phase 2 (reference population) and before Phase 3
//! (reachability) so any reference attached here participates in reachability
//! and re-export chain propagation downstream.

use rustc_hash::FxHashMap;

use fallow_types::discover::FileId;
use fallow_types::extract::{ImportedName, NamespaceObjectAlias};

use crate::resolve::{ResolveResult, ResolvedModule};

use super::ModuleGraph;
use super::narrowing::{
    create_synthetic_exports_for_star_re_exports, mark_member_exports_referenced,
};
use super::types::ReferenceKind;

/// One credit operation collected during the scan and applied after the loop
/// to keep mutable borrows of `ModuleGraph::modules` localised.
struct PendingCredit {
    /// Index into `ModuleGraph::modules` of the namespace target file.
    target_module_idx: usize,
    /// Member name to credit on the target's exports.
    member: String,
    /// Consumer file that produced the access.
    consumer_file_id: FileId,
    /// Span of the consumer's import that brought the aliased export into scope.
    import_span: oxc_span::Span,
}

/// Propagate cross-package consumer accesses through `NamespaceObjectAlias`
/// entries on each `ResolvedModule`. Mutates `graph.modules[*].exports` to
/// attach a `SymbolReference` for each accessed member on the namespace's
/// source file.
pub(super) fn propagate_cross_package_aliases(
    graph: &mut ModuleGraph,
    module_by_id: &FxHashMap<FileId, &ResolvedModule>,
) {
    let pending = collect_pending_credits(graph, module_by_id);
    apply_pending_credits(graph, &pending);
}

fn collect_pending_credits(
    graph: &ModuleGraph,
    module_by_id: &FxHashMap<FileId, &ResolvedModule>,
) -> Vec<PendingCredit> {
    let mut pending = Vec::new();

    for alias_module in module_by_id.values() {
        if alias_module.namespace_object_aliases.is_empty() {
            continue;
        }
        let alias_file_id = alias_module.file_id;
        for alias in &alias_module.namespace_object_aliases {
            let Some(namespace_target_id) = resolve_namespace_target(alias_module, alias) else {
                continue;
            };
            let Some(target_module_idx) = module_index_for_file(graph, namespace_target_id) else {
                continue;
            };
            collect_credits_for_alias(
                module_by_id,
                alias_file_id,
                alias,
                target_module_idx,
                &mut pending,
            );
        }
    }

    pending
}

/// Resolve the file_id of a namespace import on `alias_module` whose local
/// name matches `alias.namespace_local`. Only `InternalModule` targets count;
/// external packages cannot have references propagated.
fn resolve_namespace_target(
    alias_module: &ResolvedModule,
    alias: &NamespaceObjectAlias,
) -> Option<FileId> {
    alias_module.resolved_imports.iter().find_map(|import| {
        if import.info.local_name != alias.namespace_local {
            return None;
        }
        if !matches!(import.info.imported_name, ImportedName::Namespace) {
            return None;
        }
        match &import.target {
            ResolveResult::InternalModule(file_id) => Some(*file_id),
            _ => None,
        }
    })
}

fn module_index_for_file(graph: &ModuleGraph, file_id: FileId) -> Option<usize> {
    let idx = file_id.0 as usize;
    if idx >= graph.modules.len() {
        return None;
    }
    Some(idx)
}

fn collect_credits_for_alias(
    module_by_id: &FxHashMap<FileId, &ResolvedModule>,
    alias_file_id: FileId,
    alias: &NamespaceObjectAlias,
    target_module_idx: usize,
    pending: &mut Vec<PendingCredit>,
) {
    let prefix_match = format!(".{}", alias.suffix);
    for consumer in module_by_id.values() {
        if consumer.file_id == alias_file_id {
            continue;
        }
        for import in &consumer.resolved_imports {
            if !matches!(&import.target, ResolveResult::InternalModule(file_id) if *file_id == alias_file_id)
            {
                continue;
            }
            let imported_matches = match &import.info.imported_name {
                ImportedName::Named(n) => n == &alias.via_export_name,
                ImportedName::Default => alias.via_export_name == "default",
                _ => false,
            };
            if !imported_matches {
                continue;
            }
            let consumer_local = import.info.local_name.as_str();
            if consumer_local.is_empty() {
                continue;
            }
            let expected_object = format!("{consumer_local}{prefix_match}");
            for access in &consumer.member_accesses {
                if access.object != expected_object {
                    continue;
                }
                pending.push(PendingCredit {
                    target_module_idx,
                    member: access.member.clone(),
                    consumer_file_id: consumer.file_id,
                    import_span: import.info.span,
                });
            }
        }
    }
}

/// Apply collected credits, grouping by `(target_module_idx, consumer, import_span)`
/// so each (consumer file, namespace target) pair runs through the same
/// `mark_member_exports_referenced` plus `create_synthetic_exports_for_star_re_exports`
/// pipeline that `narrow_namespace_references` uses for direct namespace
/// imports. The synthetic-export step is what handles the case where the
/// namespace target is a star barrel (`export * from './bar'`): missing
/// member exports are stubbed so Phase 4 chain resolution can propagate the
/// reference to the real defining file.
fn apply_pending_credits(graph: &mut ModuleGraph, pending: &[PendingCredit]) {
    type GroupKey = (usize, FileId, oxc_span::Span);

    let mut groups: FxHashMap<GroupKey, Vec<String>> = FxHashMap::default();
    for credit in pending {
        groups
            .entry((
                credit.target_module_idx,
                credit.consumer_file_id,
                credit.import_span,
            ))
            .or_default()
            .push(credit.member.clone());
    }

    for ((target_module_idx, consumer_file_id, import_span), members) in groups {
        let module = &mut graph.modules[target_module_idx];
        let found_members = mark_member_exports_referenced(
            &mut module.exports,
            consumer_file_id,
            &members,
            import_span,
            ReferenceKind::NamespaceImport,
        );
        create_synthetic_exports_for_star_re_exports(
            &mut module.exports,
            &module.re_exports,
            consumer_file_id,
            &members,
            &found_members,
            import_span,
        );
    }
}
