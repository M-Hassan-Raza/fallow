//! `fallow setup-hooks`: generate Claude Code PreToolUse gate files for
//! this repo (and optionally an AGENTS.md fallback block for Codex).
//!
//! The gate intercepts Claude's Bash tool when the command is `git commit`
//! or `git push`, runs `fallow audit --format json --quiet --explain`, and
//! blocks only on `verdict: "fail"`. The audit JSON is written to stderr so
//! the agent can read `_meta.docs` links and `actions`, fix the findings,
//! and retry.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Agent target for generated hook artifacts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum HookAgentArg {
    /// Claude Code: project-level `.claude/settings.json` + hook script.
    Claude,
    /// Codex: managed block appended to `AGENTS.md`.
    Codex,
}

/// CLI options for `fallow setup-hooks`.
pub struct SetupHooksOptions<'a> {
    pub root: &'a Path,
    pub agent: Option<HookAgentArg>,
    pub dry_run: bool,
    pub force: bool,
    pub user: bool,
    pub gitignore_claude: bool,
}

/// Static content of `.claude/hooks/fallow-gate.sh`.
pub const FALLOW_GATE_SCRIPT: &str = include_str!("setup_hooks/fallow-gate.sh");

/// Static content of `.claude/settings.json` when no prior file exists.
pub const CLAUDE_SETTINGS_DEFAULT: &str = include_str!("setup_hooks/settings.json");

/// Managed-block start marker for the AGENTS.md Codex fallback.
pub const AGENTS_BLOCK_START: &str = "<!-- fallow:setup-hooks:start -->";
/// Managed-block end marker for the AGENTS.md Codex fallback.
pub const AGENTS_BLOCK_END: &str = "<!-- fallow:setup-hooks:end -->";

/// Body of the managed block that `fallow setup-hooks` maintains in
/// `AGENTS.md`. The `start` and `end` markers bracket this body so re-runs
/// replace it in place rather than appending duplicates.
pub const AGENTS_BLOCK_BODY: &str = "## Fallow local gate

Before any `git commit` or `git push`, run `fallow audit --format json --quiet --explain`. If the verdict is `fail`, fix the reported findings before retrying. Treat JSON runtime errors like `{ \"error\": true, ... }` as non-blocking.
";

/// Entry point for the `fallow setup-hooks` subcommand.
pub fn run_setup_hooks(opts: &SetupHooksOptions<'_>) -> ExitCode {
    let plan = match Plan::resolve(opts) {
        Ok(plan) => plan,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };

    if plan.is_empty() {
        eprintln!(
            "No .claude/, AGENTS.md, or .codex/ found; pass --agent claude or --agent codex to force a target."
        );
        return ExitCode::from(2);
    }

    if opts.dry_run {
        plan.print_dry_run();
        return ExitCode::SUCCESS;
    }

    if let Err(msg) = plan.execute(opts) {
        eprintln!("{msg}");
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

/// Resolved write plan derived from [`SetupHooksOptions`] and the filesystem.
#[derive(Debug, Default)]
struct Plan {
    claude: Option<ClaudeTargets>,
    codex: Option<CodexTargets>,
}

#[derive(Debug)]
struct ClaudeTargets {
    settings_path: PathBuf,
    script_path: PathBuf,
}

#[derive(Debug)]
struct CodexTargets {
    agents_path: PathBuf,
}

impl Plan {
    fn resolve(opts: &SetupHooksOptions<'_>) -> Result<Self, String> {
        let (want_claude, want_codex) = match opts.agent {
            Some(HookAgentArg::Claude) => (true, false),
            Some(HookAgentArg::Codex) => (false, true),
            None => auto_detect(opts.root),
        };

        let mut plan = Self::default();
        if want_claude {
            plan.claude = Some(ClaudeTargets::resolve(opts)?);
        }
        if want_codex {
            plan.codex = Some(CodexTargets::resolve(opts));
        }
        Ok(plan)
    }

    fn is_empty(&self) -> bool {
        self.claude.is_none() && self.codex.is_none()
    }

    fn print_dry_run(&self) {
        eprintln!("fallow setup-hooks (dry run)");
        if let Some(claude) = &self.claude {
            eprintln!();
            eprintln!("Would write: {}", claude.settings_path.display());
            eprintln!("Would write: {}", claude.script_path.display());
        }
        if let Some(codex) = &self.codex {
            eprintln!();
            eprintln!(
                "Would append managed block to: {}",
                codex.agents_path.display()
            );
        }
    }

    fn execute(&self, opts: &SetupHooksOptions<'_>) -> Result<(), String> {
        if let Some(claude) = &self.claude {
            claude.execute(opts)?;
        }
        if let Some(codex) = &self.codex {
            codex.execute()?;
        }
        if self.claude.is_some() && opts.gitignore_claude {
            ensure_gitignore_entry(opts.root, ".claude/")
                .map_err(|e| format!("Failed to update .gitignore for .claude/: {e}"))?;
        }
        Ok(())
    }
}

/// Detect which surfaces are present in `root`.
///
/// Returns `(want_claude, want_codex)`. Falls back to Claude when neither
/// surface is present so the tool always produces a useful default.
fn auto_detect(root: &Path) -> (bool, bool) {
    let has_claude = root.join(".claude").is_dir();
    let has_codex = root.join("AGENTS.md").is_file() || root.join(".codex").is_dir();
    if !has_claude && !has_codex {
        return (true, false);
    }
    (has_claude, has_codex)
}

impl ClaudeTargets {
    fn resolve(opts: &SetupHooksOptions<'_>) -> Result<Self, String> {
        let base = if opts.user {
            home_dir().ok_or_else(|| {
                "Cannot resolve user home directory; unset --user or set $HOME.".to_string()
            })?
        } else {
            opts.root.to_path_buf()
        };
        Ok(Self {
            settings_path: base.join(".claude").join("settings.json"),
            script_path: base.join(".claude").join("hooks").join("fallow-gate.sh"),
        })
    }

    fn execute(&self, opts: &SetupHooksOptions<'_>) -> Result<(), String> {
        if let Some(parent) = self.settings_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        if let Some(parent) = self.script_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }

        merge_claude_settings(&self.settings_path, opts.force)?;
        write_executable_script(&self.script_path, FALLOW_GATE_SCRIPT, opts.force)?;

        eprintln!("Wrote {}", self.settings_path.display());
        eprintln!("Wrote {}", self.script_path.display());
        Ok(())
    }
}

impl CodexTargets {
    fn resolve(opts: &SetupHooksOptions<'_>) -> Self {
        Self {
            agents_path: opts.root.join("AGENTS.md"),
        }
    }

    fn execute(&self) -> Result<(), String> {
        upsert_managed_block(&self.agents_path)
            .map_err(|e| format!("Failed to update {}: {e}", self.agents_path.display()))?;
        eprintln!("Updated managed block in {}", self.agents_path.display());
        Ok(())
    }
}

/// Merge the default Claude settings into an existing `settings.json` (or
/// write the file fresh if none exists). Preserves unrelated top-level keys
/// and avoids duplicate handlers on repeat runs.
fn merge_claude_settings(path: &Path, force: bool) -> Result<(), String> {
    let existing_raw = std::fs::read_to_string(path).ok();
    let desired: serde_json::Value = serde_json::from_str(CLAUDE_SETTINGS_DEFAULT)
        .map_err(|e| format!("internal default settings.json is invalid: {e}"))?;

    let merged = match existing_raw {
        None => desired,
        Some(raw) if raw.trim().is_empty() => desired,
        Some(raw) => {
            let current: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    if !force {
                        return Err(format!(
                            "{} is not valid JSON ({e}); re-run with --force to overwrite.",
                            path.display()
                        ));
                    }
                    desired.clone()
                }
            };
            merge_settings_value(current, &desired)?
        }
    };

    let serialized = serde_json::to_string_pretty(&merged)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    let mut content = serialized;
    content.push('\n');
    std::fs::write(path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Merge desired hook handlers into an existing settings `serde_json::Value`.
///
/// Adds the `$schema`, ensures `hooks.PreToolUse` exists as an array, finds or
/// creates the `{"matcher": "Bash"}` group, and appends any desired handlers
/// whose `(type, if, command)` triple is not already present.
fn merge_settings_value(
    mut current: serde_json::Value,
    desired: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let current_obj = current
        .as_object_mut()
        .ok_or_else(|| "settings.json must be a JSON object".to_string())?;

    if !current_obj.contains_key("$schema")
        && let Some(schema) = desired.get("$schema").cloned()
    {
        current_obj.insert("$schema".to_string(), schema);
    }

    let hooks_entry = current_obj
        .entry("hooks".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let hooks_obj = hooks_entry
        .as_object_mut()
        .ok_or_else(|| "settings.json `hooks` must be a JSON object".to_string())?;

    let pretool_entry = hooks_obj
        .entry("PreToolUse".to_string())
        .or_insert_with(|| serde_json::json!([]));
    let pretool_arr = pretool_entry
        .as_array_mut()
        .ok_or_else(|| "settings.json `hooks.PreToolUse` must be an array".to_string())?;

    let desired_handlers: Vec<serde_json::Value> = desired
        .get("hooks")
        .and_then(|h| h.get("PreToolUse"))
        .and_then(|p| p.as_array())
        .and_then(|groups| groups.first())
        .and_then(|group| group.get("hooks"))
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();

    let group_idx = pretool_arr
        .iter()
        .position(|group| group.get("matcher").and_then(serde_json::Value::as_str) == Some("Bash"));

    match group_idx {
        Some(idx) => {
            let group = pretool_arr[idx]
                .as_object_mut()
                .ok_or_else(|| "PreToolUse group must be a JSON object".to_string())?;
            let group_hooks = group
                .entry("hooks".to_string())
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .ok_or_else(|| "PreToolUse group `hooks` must be an array".to_string())?;
            for handler in desired_handlers {
                if !group_hooks
                    .iter()
                    .any(|existing| handlers_match(existing, &handler))
                {
                    group_hooks.push(handler);
                }
            }
        }
        None => {
            pretool_arr.push(serde_json::json!({
                "matcher": "Bash",
                "hooks": desired_handlers,
            }));
        }
    }

    Ok(current)
}

fn handlers_match(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    let key = |v: &serde_json::Value| {
        (
            v.get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            v.get("if")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            v.get("command")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        )
    };
    key(left) == key(right)
}

/// Write an executable shell script. On Unix sets mode `0o755`. Refuses to
/// clobber an existing file with different contents unless `force` is set.
fn write_executable_script(path: &Path, content: &str, force: bool) -> Result<(), String> {
    if path.exists() {
        let existing = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if existing == content {
            set_executable_bit(path);
            return Ok(());
        }
        if !force {
            return Err(format!(
                "{} already exists and differs; re-run with --force to overwrite.",
                path.display()
            ));
        }
    }
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    drop(file);
    set_executable_bit(path);
    Ok(())
}

#[cfg(unix)]
fn set_executable_bit(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(mut perms) = std::fs::metadata(path).map(|m| m.permissions()) {
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn set_executable_bit(_path: &Path) {
    // Windows: no executable bit; `bash` runs the script via its own shebang.
}

/// Append or replace the managed Codex block in `AGENTS.md`. Idempotent.
fn upsert_managed_block(path: &Path) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let new_block = format!("{AGENTS_BLOCK_START}\n{AGENTS_BLOCK_BODY}{AGENTS_BLOCK_END}\n");

    let next = if let (Some(start), Some(end)) = (
        existing.find(AGENTS_BLOCK_START),
        existing.find(AGENTS_BLOCK_END),
    ) {
        let end_line_end = existing[end..]
            .find('\n')
            .map_or(existing.len(), |offset| end + offset + 1);
        let mut buf = String::with_capacity(existing.len() + new_block.len());
        buf.push_str(&existing[..start]);
        buf.push_str(&new_block);
        buf.push_str(&existing[end_line_end..]);
        buf
    } else if existing.is_empty() {
        new_block
    } else {
        let mut buf = existing;
        if !buf.ends_with('\n') {
            buf.push('\n');
        }
        buf.push('\n');
        buf.push_str(&new_block);
        buf
    };

    std::fs::write(path, next)
}

fn ensure_gitignore_entry(root: &Path, entry: &str) -> std::io::Result<()> {
    let gitignore_path = root.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    let target = entry.trim_end_matches('/');
    let already_ignored = existing.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == target || trimmed == entry
    });
    if already_ignored {
        return Ok(());
    }

    let is_new = existing.is_empty();
    let mut contents = existing;
    if !is_new && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(entry);
    if !entry.ends_with('\n') {
        contents.push('\n');
    }
    std::fs::write(&gitignore_path, contents)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn opts(root: &Path) -> SetupHooksOptions<'_> {
        SetupHooksOptions {
            root,
            agent: None,
            dry_run: false,
            force: false,
            user: false,
            gitignore_claude: false,
        }
    }

    #[test]
    fn auto_defaults_to_claude_when_no_surface_exists() {
        let tmp = tempdir().unwrap();
        let (claude, codex) = auto_detect(tmp.path());
        assert!(claude);
        assert!(!codex);
    }

    #[test]
    fn auto_picks_both_when_claude_and_agents_present() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "# agents\n").unwrap();
        let (claude, codex) = auto_detect(tmp.path());
        assert!(claude);
        assert!(codex);
    }

    #[test]
    fn dry_run_does_not_touch_files() {
        let tmp = tempdir().unwrap();
        let mut o = opts(tmp.path());
        o.dry_run = true;
        o.agent = Some(HookAgentArg::Claude);
        let code = run_setup_hooks(&o);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(!tmp.path().join(".claude").exists());
    }

    #[test]
    fn claude_flow_writes_both_files() {
        let tmp = tempdir().unwrap();
        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        let code = run_setup_hooks(&o);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(tmp.path().join(".claude/settings.json").is_file());
        assert!(tmp.path().join(".claude/hooks/fallow-gate.sh").is_file());
    }

    #[test]
    fn settings_merge_preserves_unrelated_keys() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        let existing = r#"{"env":{"FOO":"bar"},"hooks":{"PostToolUse":[]}}"#;
        std::fs::write(tmp.path().join(".claude/settings.json"), existing).unwrap();

        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);

        let result = std::fs::read_to_string(tmp.path().join(".claude/settings.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["env"]["FOO"], "bar");
        assert!(parsed["hooks"]["PostToolUse"].is_array());
        let pretool = parsed["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pretool.len(), 1);
        assert_eq!(pretool[0]["matcher"], "Bash");
    }

    #[test]
    fn settings_merge_is_idempotent() {
        let tmp = tempdir().unwrap();
        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        let first = std::fs::read_to_string(tmp.path().join(".claude/settings.json")).unwrap();
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        let second = std::fs::read_to_string(tmp.path().join(".claude/settings.json")).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn script_refuses_to_clobber_without_force() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/hooks")).unwrap();
        let script_path = tmp.path().join(".claude/hooks/fallow-gate.sh");
        std::fs::write(&script_path, "#!/bin/sh\necho different\n").unwrap();

        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        let code = run_setup_hooks(&o);
        assert_eq!(code, ExitCode::from(2));
        let kept = std::fs::read_to_string(&script_path).unwrap();
        assert_eq!(kept, "#!/bin/sh\necho different\n");
    }

    #[test]
    fn script_overwrites_with_force() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/hooks")).unwrap();
        let script_path = tmp.path().join(".claude/hooks/fallow-gate.sh");
        std::fs::write(&script_path, "#!/bin/sh\necho different\n").unwrap();

        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        o.force = true;
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        let replaced = std::fs::read_to_string(&script_path).unwrap();
        assert_eq!(replaced, FALLOW_GATE_SCRIPT);
    }

    #[test]
    fn agents_block_appends_once() {
        let tmp = tempdir().unwrap();
        let agents_path = tmp.path().join("AGENTS.md");
        std::fs::write(&agents_path, "# Project agents\n").unwrap();

        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Codex);
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        let after_first = std::fs::read_to_string(&agents_path).unwrap();
        assert_eq!(after_first.matches(AGENTS_BLOCK_START).count(), 1);

        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        let after_second = std::fs::read_to_string(&agents_path).unwrap();
        assert_eq!(after_second, after_first);
    }

    #[test]
    fn agents_block_replaces_managed_section_in_place() {
        let tmp = tempdir().unwrap();
        let agents_path = tmp.path().join("AGENTS.md");
        let seeded =
            format!("# agents\n\n{AGENTS_BLOCK_START}\nstale body\n{AGENTS_BLOCK_END}\n\nbelow\n");
        std::fs::write(&agents_path, seeded).unwrap();

        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Codex);
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);

        let contents = std::fs::read_to_string(&agents_path).unwrap();
        assert!(contents.contains("Fallow local gate"));
        assert!(!contents.contains("stale body"));
        assert!(contents.contains("below"));
    }

    #[test]
    fn gitignore_unchanged_by_default() {
        let tmp = tempdir().unwrap();
        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        assert!(!tmp.path().join(".gitignore").exists());
    }

    #[test]
    fn gitignore_updates_only_with_flag() {
        let tmp = tempdir().unwrap();
        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        o.gitignore_claude = true;
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        let ignored = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(ignored.contains(".claude/"));
    }

    #[cfg(unix)]
    #[test]
    fn hook_script_is_executable_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempdir().unwrap();
        let mut o = opts(tmp.path());
        o.agent = Some(HookAgentArg::Claude);
        assert_eq!(run_setup_hooks(&o), ExitCode::SUCCESS);
        let mode = std::fs::metadata(tmp.path().join(".claude/hooks/fallow-gate.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "expected executable bits set");
    }
}
