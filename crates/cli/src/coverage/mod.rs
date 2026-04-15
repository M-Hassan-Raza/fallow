//! `fallow coverage setup` — first-run resumable state machine for the paid
//! production-coverage analyzer.
//!
//! The full state machine (license check → sidecar install → framework-aware
//! recipe → run analysis) is the next implementation pass. This skeleton
//! prints the recipe and the install commands so the spec's documented UX is
//! discoverable today.

use std::process::ExitCode;

use fallow_license::{DEFAULT_HARD_FAIL_DAYS, LicenseStatus};

use crate::license::{PUBLIC_KEY_BYTES, verifying_key};

/// Subcommands for `fallow coverage`.
#[derive(Debug, Clone, Copy)]
pub enum CoverageSubcommand {
    /// Resumable first-run setup flow.
    Setup,
}

/// Dispatch a `fallow coverage <sub>` invocation.
pub fn run(subcommand: CoverageSubcommand) -> ExitCode {
    match subcommand {
        CoverageSubcommand::Setup => run_setup(),
    }
}

fn run_setup() -> ExitCode {
    println!("fallow coverage setup");
    println!();
    println!("What \"production coverage\" means: fallow looks at which functions actually");
    println!("ran in your deployed app, so it can say \"this code is never called\" with");
    println!("proof, not just \"this code has no static references.\"");
    println!();

    // Step 1: license
    let key = match verifying_key() {
        Ok(k) => k,
        Err(msg) => {
            eprintln!("fallow coverage setup: {msg}");
            return ExitCode::from(2);
        }
    };
    let license_status = fallow_license::load_and_verify(&key, DEFAULT_HARD_FAIL_DAYS);
    let license_ok = matches!(
        license_status,
        Ok(LicenseStatus::Valid { .. } | LicenseStatus::ExpiredWarning { .. })
    );
    if license_ok {
        println!("Step 1/4: License: ✓");
    } else {
        println!("Step 1/4: License: not active");
        println!("  → Run: fallow license activate --trial --email you@company.com");
    }

    // Step 2: sidecar
    let sidecar_present = sidecar_discoverable();
    if sidecar_present {
        println!("Step 2/4: Sidecar (fallow-cov): ✓");
    } else {
        println!("Step 2/4: Sidecar (fallow-cov): not installed");
        println!("  → Run: npm install -g @fallow-cli/fallow-cov");
        println!("  → Or:  download from https://github.com/fallow-rs/fallow-cloud/releases");
    }

    // Step 3: coverage recipe (placeholder — framework detection is follow-up)
    println!("Step 3/4: Collect coverage from your app (e.g., NODE_V8_COVERAGE=./coverage node …)");
    println!("  → Re-run this command after producing coverage data.");

    // Step 4: run analysis
    println!("Step 4/4: fallow health --production-coverage ./coverage/  (wires up next pass)");

    if license_ok && sidecar_present {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

/// Detect whether `fallow-cov` is reachable.
///
/// Discovery order matches the spec: explicit env override > canonical install
/// path > `$PATH`. Windows `.exe` suffix is tried implicitly by the OS.
fn sidecar_discoverable() -> bool {
    if let Ok(path) = std::env::var("FALLOW_COV_BIN") {
        return std::path::Path::new(&path).exists();
    }
    let canonical = canonical_sidecar_path();
    if canonical.exists() {
        return true;
    }
    which("fallow-cov")
}

fn canonical_sidecar_path() -> std::path::PathBuf {
    std::env::var("HOME")
        .map_or_else(|_| std::path::PathBuf::from("."), std::path::PathBuf::from)
        .join(".fallow")
        .join("bin")
        .join("fallow-cov")
}

fn which(name: &str) -> bool {
    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| {
        let candidate = dir.join(name);
        candidate.is_file() || candidate.with_extension("exe").is_file()
    })
}

// Suppress an unused-import warning when this module is included.
#[allow(dead_code, reason = "kept for future state-machine implementation")]
const _PUBLIC_KEY: [u8; 32] = PUBLIC_KEY_BYTES;
