use crate::params::AuditParams;

use super::{push_global, push_scope};

/// Build CLI arguments for the `audit` tool.
pub fn build_audit_args(params: &AuditParams) -> Vec<String> {
    let mut args = vec![
        "audit".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--quiet".to_string(),
        "--explain".to_string(),
    ];

    push_global(
        &mut args,
        params.root.as_deref(),
        params.config.as_deref(),
        params.no_cache,
        params.threads,
    );
    if let Some(ref base) = params.base {
        args.extend(["--base".to_string(), base.clone()]);
    }
    push_scope(&mut args, params.production, params.workspace.as_deref());

    args
}
