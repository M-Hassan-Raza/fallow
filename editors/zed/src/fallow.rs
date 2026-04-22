use std::fs;
use std::path::{Path, PathBuf};

use zed_extension_api::{
    self as zed, DownloadedFileType, LanguageServerId,
    LanguageServerInstallationStatus as InstallStatus, Result, make_file_executable,
    set_language_server_installation_status, settings::LspSettings,
};

const LANGUAGE_SERVER_ID: &str = "fallow";
const RELEASE_REPOSITORY: &str = "fallow-rs/fallow";
const BINARY_BASENAME: &str = "fallow-lsp";
const MANAGED_DIR_PREFIX: &str = "fallow-";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Platform {
    DarwinAarch64,
    DarwinX8664,
    LinuxAarch64Gnu,
    LinuxX8664Gnu,
    WindowsX8664,
}

impl Platform {
    fn current() -> Result<Self> {
        Self::from_parts(zed::current_platform().0, zed::current_platform().1)
    }

    fn from_parts(os: zed::Os, arch: zed::Architecture) -> Result<Self> {
        match (os, arch) {
            (zed::Os::Mac, zed::Architecture::Aarch64) => Ok(Self::DarwinAarch64),
            (zed::Os::Mac, zed::Architecture::X8664) => Ok(Self::DarwinX8664),
            (zed::Os::Linux, zed::Architecture::Aarch64) => Ok(Self::LinuxAarch64Gnu),
            (zed::Os::Linux, zed::Architecture::X8664) => Ok(Self::LinuxX8664Gnu),
            (zed::Os::Windows, zed::Architecture::X8664) => Ok(Self::WindowsX8664),
            (_, zed::Architecture::X86) => {
                Err("32-bit x86 is not supported by Fallow release binaries".to_string())
            }
            _ => Err("This platform is not supported by the Fallow Zed extension".to_string()),
        }
    }

    fn release_asset_name(self) -> &'static str {
        match self {
            Self::DarwinAarch64 => "fallow-lsp-darwin-arm64",
            Self::DarwinX8664 => "fallow-lsp-darwin-x64",
            Self::LinuxAarch64Gnu => "fallow-lsp-linux-arm64-gnu",
            Self::LinuxX8664Gnu => "fallow-lsp-linux-x64-gnu",
            Self::WindowsX8664 => "fallow-lsp-win32-x64-msvc.exe",
        }
    }

    fn executable_name(self) -> &'static str {
        match self {
            Self::WindowsX8664 => "fallow-lsp.exe",
            _ => BINARY_BASENAME,
        }
    }

    fn local_binary_candidates(self) -> &'static [&'static str] {
        match self {
            Self::WindowsX8664 => &["fallow-lsp.cmd", "fallow-lsp.exe"],
            _ => &[BINARY_BASENAME],
        }
    }

    fn needs_executable_bit(self) -> bool {
        !matches!(self, Self::WindowsX8664)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ResolvedBinary {
    path: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

#[derive(Default)]
struct FallowExtension {
    cached_binary_path: Option<String>,
}

impl FallowExtension {
    fn resolve_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<ResolvedBinary> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree).ok();
        let args = settings
            .as_ref()
            .and_then(|value| value.binary.as_ref())
            .and_then(|value| value.arguments.clone())
            .unwrap_or_default();
        let env = settings
            .as_ref()
            .and_then(|value| value.binary.as_ref())
            .and_then(|value| value.env.clone())
            .map(|items| items.into_iter().collect())
            .unwrap_or_default();

        let path = if let Some(path) = settings
            .as_ref()
            .and_then(|value| value.binary.as_ref())
            .and_then(|value| value.path.clone())
        {
            ensure_binary_exists(&path)?;
            path
        } else if let Some(path) = find_local_workspace_binary(worktree, Platform::current()?) {
            path
        } else if let Some(path) = worktree.which(BINARY_BASENAME) {
            path
        } else {
            self.managed_binary_path(language_server_id)?
        };

        Ok(ResolvedBinary { path, args, env })
    }

    fn managed_binary_path(&mut self, language_server_id: &LanguageServerId) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
                return Ok(path.clone());
            }
        }

        let platform = Platform::current()?;

        set_language_server_installation_status(
            language_server_id,
            &InstallStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            RELEASE_REPOSITORY,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == platform.release_asset_name())
            .ok_or_else(|| {
                format!(
                    "No fallow-lsp asset found for {} in release {}",
                    platform.release_asset_name(),
                    release.version
                )
            })?;

        let version_dir = format!("{MANAGED_DIR_PREFIX}{}", release.version);
        let binary_path = format!("{version_dir}/{}", platform.executable_name());

        if !fs::metadata(&binary_path).is_ok_and(|metadata| metadata.is_file()) {
            fs::create_dir_all(&version_dir)
                .map_err(|error| format!("Failed to create managed binary directory: {error}"))?;

            set_language_server_installation_status(
                language_server_id,
                &InstallStatus::Downloading,
            );
            zed::download_file(
                &asset.download_url,
                &binary_path,
                DownloadedFileType::Uncompressed,
            )
            .map_err(|error| format!("Failed to download fallow-lsp: {error}"))?;

            if platform.needs_executable_bit() {
                make_file_executable(&binary_path)
                    .map_err(|error| format!("Failed to make fallow-lsp executable: {error}"))?;
            }

            cleanup_stale_managed_dirs(&version_dir);
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for FallowExtension {
    fn new() -> Self {
        Self::default()
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        if language_server_id.as_ref() != LANGUAGE_SERVER_ID {
            return Err(format!(
                "Unrecognized language server for Fallow: {language_server_id}"
            ));
        }

        let binary = self.resolve_binary(language_server_id, worktree)?;
        Ok(zed::Command {
            command: binary.path,
            args: binary.args,
            env: binary.env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        let options = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.initialization_options.clone())
            .unwrap_or_default();
        Ok(Some(options))
    }
}

fn ensure_binary_exists(path: &str) -> Result<()> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        Ok(())
    } else {
        Err(format!(
            "Configured fallow-lsp binary does not exist: {path}"
        ))
    }
}

fn find_local_workspace_binary(worktree: &zed::Worktree, platform: Platform) -> Option<String> {
    let root_path = worktree.root_path();
    let root = Path::new(&root_path);
    find_local_workspace_binary_path(root, platform).map(path_to_string)
}

fn find_local_workspace_binary_path(root: &Path, platform: Platform) -> Option<PathBuf> {
    let bin_dir = root.join("node_modules").join(".bin");
    for candidate in platform.local_binary_candidates() {
        let path = bin_dir.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn cleanup_stale_managed_dirs(keep_dir: &str) {
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };

        if !should_remove_stale_managed_entry(&name, keep_dir) {
            continue;
        }

        if path.is_dir() {
            let _ = fs::remove_dir_all(path);
        } else {
            let _ = fs::remove_file(path);
        }
    }
}

fn should_remove_stale_managed_entry(name: &str, keep_dir: &str) -> bool {
    name.starts_with(MANAGED_DIR_PREFIX) && name != keep_dir
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

zed::register_extension!(FallowExtension);

#[cfg(test)]
mod tests {
    use super::{Platform, find_local_workspace_binary_path, should_remove_stale_managed_entry};
    use std::fs;
    use std::path::PathBuf;
    use zed_extension_api::{Architecture, Os};

    #[test]
    fn maps_release_asset_names() {
        assert_eq!(
            Platform::from_parts(Os::Mac, Architecture::Aarch64)
                .expect("mac arm64 platform should resolve")
                .release_asset_name(),
            "fallow-lsp-darwin-arm64"
        );
        assert_eq!(
            Platform::from_parts(Os::Mac, Architecture::X8664)
                .expect("mac x64 platform should resolve")
                .release_asset_name(),
            "fallow-lsp-darwin-x64"
        );
        assert_eq!(
            Platform::from_parts(Os::Linux, Architecture::X8664)
                .expect("linux x64 platform should resolve")
                .release_asset_name(),
            "fallow-lsp-linux-x64-gnu"
        );
        assert_eq!(
            Platform::from_parts(Os::Windows, Architecture::X8664)
                .expect("windows x64 platform should resolve")
                .release_asset_name(),
            "fallow-lsp-win32-x64-msvc.exe"
        );
    }

    #[test]
    fn rejects_unsupported_x86() {
        let error = Platform::from_parts(Os::Linux, Architecture::X86)
            .expect_err("32-bit x86 should not be supported");
        assert!(error.contains("32-bit x86"), "unexpected error: {error}");
    }

    #[test]
    fn finds_local_workspace_binary_for_unix_and_windows() {
        let root = unique_temp_dir("fallow-zed-local-binary");
        let bin_dir = root.join("node_modules").join(".bin");
        fs::create_dir_all(&bin_dir).expect("failed to create node_modules/.bin");

        let unix_binary = bin_dir.join("fallow-lsp");
        fs::write(&unix_binary, "#!/bin/sh\n").expect("failed to write unix binary");
        assert_eq!(
            find_local_workspace_binary_path(&root, Platform::DarwinAarch64),
            Some(unix_binary)
        );

        let windows_binary = bin_dir.join("fallow-lsp.cmd");
        fs::write(&windows_binary, "@echo off\r\n").expect("failed to write windows binary");
        assert_eq!(
            find_local_workspace_binary_path(&root, Platform::WindowsX8664),
            Some(windows_binary)
        );

        fs::remove_dir_all(&root).expect("failed to clean temp dir");
    }

    #[test]
    fn stale_cleanup_only_targets_managed_dirs() {
        assert!(should_remove_stale_managed_entry(
            "fallow-v2.44.0",
            "fallow-v2.45.0"
        ));
        assert!(!should_remove_stale_managed_entry(
            "fallow-v2.45.0",
            "fallow-v2.45.0"
        ));
        assert!(!should_remove_stale_managed_entry(
            "other-extension",
            "fallow-v2.45.0"
        ));
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }
}
