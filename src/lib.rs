use std::fs;
use zed_extension_api::{self as zed, LanguageServerId, Result};

struct RumdlExtension {
    cached_binary_path: Option<String>,
}

impl RumdlExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        // Check if we have a cached path that still exists
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map_or(false, |m| m.is_file()) {
                return Ok(path.clone());
            }
        }

        // Check if rumdl is available in PATH
        if let Some(path) = worktree.which("rumdl") {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        // Download from GitHub releases
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "rvben/rumdl",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = zed::current_platform();
        let asset_name = format!(
            "rumdl-{arch}-{os}.{ext}",
            arch = match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X86 => "x86",
                zed::Architecture::X8664 => "x86_64",
            },
            os = match platform {
                zed::Os::Mac => "apple-darwin",
                zed::Os::Linux => "unknown-linux-gnu",
                zed::Os::Windows => "pc-windows-msvc",
            },
            ext = match platform {
                zed::Os::Windows => "zip",
                _ => "tar.gz",
            }
        );

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("rumdl-{}", release.version);
        let binary_path = format!(
            "{version_dir}/rumdl{ext}",
            ext = match platform {
                zed::Os::Windows => ".exe",
                _ => "",
            }
        );

        if !fs::metadata(&binary_path).map_or(false, |m| m.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(
                &asset.download_url,
                &version_dir,
                match platform {
                    zed::Os::Windows => zed::DownloadedFileType::Zip,
                    _ => zed::DownloadedFileType::GzipTar,
                },
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path)?;

            // Clean up old versions
            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.to_string_lossy().starts_with("rumdl-")
                    && entry_path.to_string_lossy() != version_dir
                {
                    fs::remove_dir_all(&entry_path).ok();
                }
            }
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for RumdlExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary_path = self.language_server_binary_path(language_server_id, worktree)?;

        Ok(zed::Command {
            command: binary_path,
            args: vec!["server".to_string()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(RumdlExtension);
