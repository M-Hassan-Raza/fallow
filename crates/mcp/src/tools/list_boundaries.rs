use crate::params::ListBoundariesParams;

pub fn build_list_boundaries_args(params: &ListBoundariesParams) -> Vec<String> {
    let mut args = vec![
        "list".to_string(),
        "--boundaries".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--quiet".to_string(),
    ];

    if let Some(ref root) = params.root {
        args.push("--root".to_string());
        args.push(root.clone());
    }
    if let Some(ref config) = params.config {
        args.push("--config".to_string());
        args.push(config.clone());
    }
    if params.no_cache == Some(true) {
        args.push("--no-cache".to_string());
    }
    if let Some(threads) = params.threads {
        args.push("--threads".to_string());
        args.push(threads.to_string());
    }

    args
}
