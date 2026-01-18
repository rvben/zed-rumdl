use std::fs;

use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

struct RumdlExtension {
    cached_binary_path: Option<String>,
    use_system_binary: bool,
}

impl RumdlExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        // Check if we have a cached path that still exists
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        // Check LSP settings for user-configured binary path
        let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;

        if let Some(binary_settings) = lsp_settings.binary {
            if let Some(path) = binary_settings.path {
                if !path.is_empty() {
                    self.cached_binary_path = Some(path.clone());
                    self.use_system_binary = true;
                    return Ok(path);
                }
            }
        }

        // Check if rumdl is available in PATH
        if let Some(path) = worktree.which("rumdl") {
            self.cached_binary_path = Some(path.clone());
            self.use_system_binary = true;
            return Ok(path);
        }

        // Download from GitHub releases
        self.use_system_binary = false;
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

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
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
                let entry_name = entry_path.to_string_lossy();
                if entry_name.starts_with("rumdl-") && entry_name != version_dir {
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
            use_system_binary: false,
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
            env: if self.use_system_binary {
                worktree.shell_env()
            } else {
                Default::default()
            },
        })
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        let settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone());
        Ok(settings)
    }
}

zed::register_extension!(RumdlExtension);

#[cfg(test)]
mod tests {
    use super::*;
    use zed_extension_api::Extension;

    #[test]
    fn test_new_extension_initial_state() {
        let ext = RumdlExtension::new();
        assert!(
            ext.cached_binary_path.is_none(),
            "A new extension instance should have no cached binary path"
        );
        assert!(
            !ext.use_system_binary,
            "A new extension instance should not use system binary by default"
        );
    }
}
