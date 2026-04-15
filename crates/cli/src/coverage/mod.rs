//! `fallow coverage setup` - resumable first-run state machine for the paid
//! production-coverage analyzer.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use fallow_config::PackageJson;
use fallow_license::{DEFAULT_HARD_FAIL_DAYS, LicenseStatus};

use crate::health::coverage as production_coverage;
use crate::license;

const COVERAGE_DOCS_URL: &str = "https://fallow.tools/coverage";

/// Subcommands for `fallow coverage`.
#[derive(Debug, Clone)]
pub enum CoverageSubcommand {
    /// Resumable first-run setup flow.
    Setup(SetupArgs),
}

/// Arguments for `fallow coverage setup`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupArgs {
    /// Accept all prompts automatically.
    pub yes: bool,
    /// Print instructions instead of prompting.
    pub non_interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameworkKind {
    NextJs,
    NestJs,
    PlainNode,
    Other,
}

impl FrameworkKind {
    const fn label(self) -> &'static str {
        match self {
            Self::NextJs => "Next.js project",
            Self::NestJs => "NestJS project",
            Self::PlainNode => "plain Node project",
            Self::Other => "custom project",
        }
    }
}

/// Dispatch a `fallow coverage <sub>` invocation.
#[expect(
    clippy::needless_pass_by_value,
    reason = "command dispatch consumes the mapped clap subcommand value"
)]
pub fn run(subcommand: CoverageSubcommand, root: &Path) -> ExitCode {
    match subcommand {
        CoverageSubcommand::Setup(args) => run_setup(args, root),
    }
}

fn run_setup(args: SetupArgs, root: &Path) -> ExitCode {
    println!("fallow coverage setup");
    println!();
    println!("What \"production coverage\" means: fallow looks at which functions actually");
    println!("ran in your deployed app, so it can say \"this code is never called\" with");
    println!("proof, not just \"this code has no static references.\"");
    println!();

    let key = match license::verifying_key() {
        Ok(key) => key,
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return ExitCode::from(2);
        }
    };

    let license_state = fallow_license::load_and_verify(&key, DEFAULT_HARD_FAIL_DAYS);
    if let Some(exit) = handle_license_step(root, args, &license_state) {
        return exit;
    }

    if let Some(exit) = handle_sidecar_step(args) {
        return exit;
    }

    let framework = detect_framework(root);
    let recipe_path = match write_recipe(root, framework) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return ExitCode::from(2);
        }
    };

    if let Some(coverage_path) = detect_coverage_artifact(root) {
        println!(
            "Step 3/4: Coverage found at {}",
            display_relative(root, &coverage_path)
        );
        println!(
            "Step 4/4: Running fallow health --production-coverage {} ...",
            display_relative(root, &coverage_path)
        );
        return run_health_analysis(root, &coverage_path);
    }

    println!("Step 3/4: Collecting coverage for your app.");
    println!("  -> Detected: {}.", framework.label());
    println!(
        "  -> Wrote {} with the {} recipe.",
        display_relative(root, &recipe_path),
        framework.label()
    );
    println!("  -> Run your app with the instrumentation on, then re-run this command.");
    ExitCode::SUCCESS
}

fn handle_license_step(
    root: &Path,
    args: SetupArgs,
    license_state: &Result<LicenseStatus, fallow_license::LicenseError>,
) -> Option<ExitCode> {
    match license_state {
        Ok(
            LicenseStatus::Valid { .. }
            | LicenseStatus::ExpiredWarning { .. }
            | LicenseStatus::ExpiredWatermark { .. },
        ) => {
            println!("Step 1/4: License check... ok.");
            None
        }
        Ok(LicenseStatus::Missing) => {
            println!("Step 1/4: License check... none found.");
            start_trial_if_needed(root, args)
        }
        Ok(LicenseStatus::HardFail {
            days_since_expiry, ..
        }) => {
            println!("Step 1/4: License check... expired {days_since_expiry} days ago.");
            start_trial_if_needed(root, args)
        }
        Err(err) => {
            println!("Step 1/4: License check... existing token is invalid ({err}).");
            start_trial_if_needed(root, args)
        }
    }
}

fn start_trial_if_needed(root: &Path, args: SetupArgs) -> Option<ExitCode> {
    let prompt = "  -> Start a 30-day trial (email only, no card)? [Y/n] ";
    let accepted = match confirm(prompt, args) {
        Ok(accepted) => accepted,
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return Some(ExitCode::from(2));
        }
    };
    if !accepted {
        println!("  -> Run: fallow license activate --trial --email you@company.com");
        return Some(ExitCode::SUCCESS);
    }

    let email = match prompt_email(args) {
        Ok(Some(email)) => email,
        Ok(None) => return Some(ExitCode::SUCCESS),
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return Some(ExitCode::from(2));
        }
    };

    match license::activate_trial(&email) {
        Ok(status) => {
            println!(
                "  -> This license is machine-scoped (stored at {}).",
                default_license_display(root)
            );
            println!("     Your teammates each start their own trial.");
            print_trial_status(&status);
            None
        }
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            Some(ExitCode::from(7))
        }
    }
}

fn handle_sidecar_step(args: SetupArgs) -> Option<ExitCode> {
    match production_coverage::discover_sidecar() {
        Ok(path) => {
            println!("Step 2/4: Sidecar check... ok ({})", path.to_string_lossy());
            None
        }
        Err(_) => {
            println!("Step 2/4: Sidecar check... not installed.");
            println!(
                "  -> Install path: {}",
                production_coverage::canonical_sidecar_path().display()
            );
            let prompt = "  -> Install @fallow-cli/fallow-cov via npm? [Y/n] ";
            let accepted = match confirm(prompt, args) {
                Ok(accepted) => accepted,
                Err(message) => {
                    eprintln!("fallow coverage setup: {message}");
                    return Some(ExitCode::from(2));
                }
            };
            if !accepted {
                println!("  -> Run: npm install -g @fallow-cli/fallow-cov");
                println!(
                    "  -> Manual fallback: install a signed binary and place it at {}",
                    production_coverage::canonical_sidecar_path().display()
                );
                return Some(ExitCode::SUCCESS);
            }

            match install_sidecar_via_npm() {
                Ok(path) => {
                    println!("  -> Installed at {}", path.display());
                    None
                }
                Err(message) => {
                    eprintln!("fallow coverage setup: {message}");
                    Some(ExitCode::from(4))
                }
            }
        }
    }
}

fn confirm(prompt: &str, args: SetupArgs) -> Result<bool, String> {
    if args.non_interactive {
        println!("{prompt}skipped (--non-interactive)");
        return Ok(false);
    }
    if args.yes {
        println!("{prompt}Y");
        return Ok(true);
    }

    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|err| format!("failed to flush stdout: {err}"))?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|err| format!("failed to read stdin: {err}"))?;
    let trimmed = answer.trim().to_ascii_lowercase();
    Ok(trimmed.is_empty() || trimmed == "y" || trimmed == "yes")
}

fn prompt_email(args: SetupArgs) -> Result<Option<String>, String> {
    if args.non_interactive {
        println!("  -> Run: fallow license activate --trial --email you@company.com");
        return Ok(None);
    }
    if args.yes {
        let Some(email) = default_trial_email() else {
            return Err(
                "unable to infer an email address for --yes. Run without --yes or use `fallow license activate --trial --email <addr>` first."
                    .to_owned(),
            );
        };
        println!("  -> Email: {email}");
        return Ok(Some(email));
    }

    print!("  -> Email: ");
    io::stdout()
        .flush()
        .map_err(|err| format!("failed to flush stdout: {err}"))?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|err| format!("failed to read stdin: {err}"))?;
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return Err("email is required to start a trial".to_owned());
    }
    Ok(Some(trimmed.to_owned()))
}

fn default_trial_email() -> Option<String> {
    std::env::var("EMAIL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_config_email)
}

fn git_config_email() -> Option<String> {
    let output = Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let email = String::from_utf8(output.stdout).ok()?;
    let trimmed = email.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn print_trial_status(status: &LicenseStatus) {
    match status {
        LicenseStatus::Valid {
            days_until_expiry, ..
        } => {
            println!("  -> Trial active. {days_until_expiry} days remaining.");
        }
        LicenseStatus::ExpiredWarning {
            days_since_expiry, ..
        }
        | LicenseStatus::ExpiredWatermark {
            days_since_expiry, ..
        }
        | LicenseStatus::HardFail {
            days_since_expiry, ..
        } => {
            println!(
                "  -> Trial activated, but it is already expired by {days_since_expiry} days."
            );
        }
        LicenseStatus::Missing => {
            println!("  -> Trial request completed, but no license was stored.");
        }
    }
}

fn default_license_display(root: &Path) -> String {
    display_relative(root, &fallow_license::default_license_path())
}

fn install_sidecar_via_npm() -> Result<PathBuf, String> {
    let status = Command::new("npm")
        .args(["install", "-g", "@fallow-cli/fallow-cov"])
        .status()
        .map_err(|err| format!("failed to run npm install -g @fallow-cli/fallow-cov: {err}"))?;

    if !status.success() {
        return Err(
            "npm install -g @fallow-cli/fallow-cov failed. Install it manually or place the binary in ~/.fallow/bin/fallow-cov"
                .to_owned(),
        );
    }

    production_coverage::discover_sidecar().map_err(|_| {
        format!(
            "sidecar install finished but {} is still missing",
            production_coverage::canonical_sidecar_path().display()
        )
    })
}

fn detect_framework(root: &Path) -> FrameworkKind {
    let Ok(package_json) = PackageJson::load(&root.join("package.json")) else {
        return FrameworkKind::Other;
    };
    let dependencies = package_json.all_dependency_names();
    if dependencies.iter().any(|name| name == "next") {
        FrameworkKind::NextJs
    } else if dependencies.iter().any(|name| name.starts_with("@nestjs/")) {
        FrameworkKind::NestJs
    } else if package_json.name.is_some() {
        FrameworkKind::PlainNode
    } else {
        FrameworkKind::Other
    }
}

fn write_recipe(root: &Path, framework: FrameworkKind) -> Result<PathBuf, String> {
    let docs_dir = root.join("docs");
    std::fs::create_dir_all(&docs_dir)
        .map_err(|err| format!("failed to create {}: {err}", docs_dir.display()))?;
    let path = docs_dir.join("collect-coverage.md");
    std::fs::write(&path, recipe_contents(framework))
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
}

fn recipe_contents(framework: FrameworkKind) -> String {
    match framework {
        FrameworkKind::NextJs => r"# Collect production coverage for Next.js

1. Remove any old dump directory: `rm -rf ./coverage`
2. Build the app: `NODE_ENV=production npm run build`
3. Start the app with V8 coverage enabled:
   `NODE_V8_COVERAGE=./coverage NODE_ENV=production npm run start`
4. Exercise the routes you care about.
5. Stop the app and run: `fallow coverage setup`
"
        .to_owned(),
        FrameworkKind::NestJs => r"# Collect production coverage for NestJS

1. Build the app first: `npm run build`
2. Remove any old dump directory: `rm -rf ./coverage`
3. Start the built server with V8 coverage enabled:
   `NODE_V8_COVERAGE=./coverage node dist/main.js`
4. Exercise your HTTP or message handlers.
5. Stop the server and run: `fallow coverage setup`
"
        .to_owned(),
        FrameworkKind::PlainNode => r"# Collect production coverage for a Node service

1. Remove any old dump directory: `rm -rf ./coverage`
2. Start the production entry point with V8 coverage enabled:
   `NODE_V8_COVERAGE=./coverage node dist/server.js`
3. Exercise the app traffic you want to analyze.
4. Stop the process and run: `fallow coverage setup`
"
        .to_owned(),
        FrameworkKind::Other => format!(
            "# Collect production coverage\n\nThis project was not matched to a built-in recipe.\nSee {COVERAGE_DOCS_URL} for framework-specific instructions.\n"
        ),
    }
}

fn detect_coverage_artifact(root: &Path) -> Option<PathBuf> {
    let coverage_dir = root.join("coverage");
    let istanbul = coverage_dir.join("coverage-final.json");
    if istanbul.is_file() {
        return Some(istanbul);
    }
    if coverage_dir.is_dir() && directory_has_json(&coverage_dir) {
        return Some(coverage_dir);
    }
    None
}

fn directory_has_json(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .any(|entry| entry.extension() == Some(OsStr::new("json")))
}

fn run_health_analysis(root: &Path, coverage_path: &Path) -> ExitCode {
    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("fallow coverage setup: failed to resolve current executable: {err}");
            return ExitCode::from(2);
        }
    };

    let status = match Command::new(current_exe)
        .arg("health")
        .arg("--root")
        .arg(root)
        .arg("--production-coverage")
        .arg(coverage_path)
        .status()
    {
        Ok(status) => status,
        Err(err) => {
            eprintln!("fallow coverage setup: failed to run health analysis: {err}");
            return ExitCode::from(2);
        }
    };

    match status.code() {
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(2)),
        None => ExitCode::from(2),
    }
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |relative| format!("./{}", relative.to_string_lossy()),
    )
}
