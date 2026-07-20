use std::fs;

use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

/// The Rust target triple rumdl uses in its release asset names for this
/// platform, or `None` if rumdl does not publish a binary for it.
fn target_triple(os: zed::Os, arch: zed::Architecture) -> Option<&'static str> {
    Some(match (os, arch) {
        (zed::Os::Mac, zed::Architecture::Aarch64) => "aarch64-apple-darwin",
        (zed::Os::Mac, zed::Architecture::X8664) => "x86_64-apple-darwin",
        (zed::Os::Linux, zed::Architecture::Aarch64) => "aarch64-unknown-linux-gnu",
        (zed::Os::Linux, zed::Architecture::X8664) => "x86_64-unknown-linux-gnu",
        (zed::Os::Windows, zed::Architecture::X8664) => "x86_64-pc-windows-msvc",
        _ => return None,
    })
}

/// The archive extension rumdl uses for this OS (`zip` on Windows, `tar.gz`
/// elsewhere).
fn asset_extension(os: zed::Os) -> &'static str {
    match os {
        zed::Os::Windows => "zip",
        _ => "tar.gz",
    }
}

/// The exact GitHub release asset name for `version` on this platform, matching
/// rumdl's `rumdl-v{version}-{target}.{ext}` release naming. `version` may carry
/// a leading `v` or not. Returns `None` for platforms rumdl does not ship.
fn asset_name(os: zed::Os, arch: zed::Architecture, version: &str) -> Option<String> {
    let triple = target_triple(os, arch)?;
    let version = version.trim_start_matches('v');
    Some(format!(
        "rumdl-v{version}-{triple}.{ext}",
        ext = asset_extension(os)
    ))
}

/// The directory a downloaded release is extracted into, relative to the
/// extension work dir.
fn version_dir(version: &str) -> String {
    format!("rumdl-{version}")
}

/// The path to the extracted rumdl binary, relative to the extension work dir.
fn binary_relative_path(version: &str, os: zed::Os) -> String {
    let exe = if matches!(os, zed::Os::Windows) {
        ".exe"
    } else {
        ""
    };
    format!("{}/rumdl{exe}", version_dir(version))
}

/// Whether `entry_name` is a leftover rumdl download directory from a version
/// other than the one currently in use, and so safe to remove.
fn is_stale_version_dir(entry_name: &str, current_version_dir: &str) -> bool {
    entry_name.starts_with("rumdl-") && entry_name != current_version_dir
}

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
        // Cached path that still exists.
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        // User-configured binary path from LSP settings.
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

        // rumdl already on PATH.
        if let Some(path) = worktree.which("rumdl") {
            self.cached_binary_path = Some(path.clone());
            self.use_system_binary = true;
            return Ok(path);
        }

        // Otherwise download the matching release binary from GitHub.
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
        let asset_name = asset_name(platform, arch, &release.version)
            .ok_or_else(|| format!("rumdl does not publish a binary for {platform:?}/{arch:?}"))?;

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {asset_name:?}"))?;

        let version_dir = version_dir(&release.version);
        let binary_path = binary_relative_path(&release.version, platform);

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

            // Remove previously downloaded versions. Match on the bare entry name
            // (`file_name`, not `path`, which would carry a `./` prefix).
            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries.flatten() {
                let name = entry.file_name();
                if is_stale_version_dir(&name.to_string_lossy(), &version_dir) {
                    fs::remove_dir_all(entry.path()).ok();
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
    use zed_extension_api::{Architecture, Extension, Os};

    /// The (OS, architecture) pairs rumdl publishes release binaries for.
    const SUPPORTED_PLATFORMS: &[(Os, Architecture)] = &[
        (Os::Mac, Architecture::Aarch64),
        (Os::Mac, Architecture::X8664),
        (Os::Linux, Architecture::Aarch64),
        (Os::Linux, Architecture::X8664),
        (Os::Windows, Architecture::X8664),
    ];

    #[test]
    fn new_extension_has_default_state() {
        let ext = RumdlExtension::new();
        assert!(ext.cached_binary_path.is_none());
        assert!(!ext.use_system_binary);
    }

    #[test]
    fn asset_name_matches_rumdl_release_naming() {
        // The exact contract with rumdl's release assets. This pins the naming
        // that a past release got wrong (the version segment was missing).
        let cases = [
            (
                Os::Mac,
                Architecture::Aarch64,
                "rumdl-v0.2.35-aarch64-apple-darwin.tar.gz",
            ),
            (
                Os::Mac,
                Architecture::X8664,
                "rumdl-v0.2.35-x86_64-apple-darwin.tar.gz",
            ),
            (
                Os::Linux,
                Architecture::Aarch64,
                "rumdl-v0.2.35-aarch64-unknown-linux-gnu.tar.gz",
            ),
            (
                Os::Linux,
                Architecture::X8664,
                "rumdl-v0.2.35-x86_64-unknown-linux-gnu.tar.gz",
            ),
            (
                Os::Windows,
                Architecture::X8664,
                "rumdl-v0.2.35-x86_64-pc-windows-msvc.zip",
            ),
        ];
        for (os, arch, expected) in cases {
            assert_eq!(asset_name(os, arch, "0.2.35").as_deref(), Some(expected));
        }
    }

    #[test]
    fn asset_name_normalizes_leading_v() {
        // A tag like "v0.2.35" and a bare "0.2.35" must produce the same asset.
        assert_eq!(
            asset_name(Os::Mac, Architecture::Aarch64, "v0.2.35"),
            asset_name(Os::Mac, Architecture::Aarch64, "0.2.35"),
        );
        assert_eq!(
            asset_name(Os::Mac, Architecture::Aarch64, "v0.2.35").as_deref(),
            Some("rumdl-v0.2.35-aarch64-apple-darwin.tar.gz"),
        );
    }

    #[test]
    fn asset_name_is_none_for_unsupported_platforms() {
        // rumdl ships no 32-bit x86 or arm64-Windows binaries.
        assert!(asset_name(Os::Linux, Architecture::X86, "0.2.35").is_none());
        assert!(asset_name(Os::Mac, Architecture::X86, "0.2.35").is_none());
        assert!(asset_name(Os::Windows, Architecture::Aarch64, "0.2.35").is_none());
    }

    #[test]
    fn every_supported_platform_resolves() {
        for &(os, arch) in SUPPORTED_PLATFORMS {
            assert!(
                target_triple(os, arch).is_some(),
                "{os:?}/{arch:?} has no target triple"
            );
            let name =
                asset_name(os, arch, "0.2.35").expect("supported platform has an asset name");
            assert!(
                name.starts_with("rumdl-v0.2.35-"),
                "unexpected name: {name}"
            );
            assert!(
                name.ends_with(".tar.gz") || name.ends_with(".zip"),
                "unexpected extension: {name}"
            );
        }
    }

    #[test]
    fn binary_path_has_exe_only_on_windows() {
        assert_eq!(
            binary_relative_path("v0.2.35", Os::Mac),
            "rumdl-v0.2.35/rumdl"
        );
        assert_eq!(
            binary_relative_path("v0.2.35", Os::Linux),
            "rumdl-v0.2.35/rumdl"
        );
        assert_eq!(
            binary_relative_path("v0.2.35", Os::Windows),
            "rumdl-v0.2.35/rumdl.exe"
        );
    }

    #[test]
    fn stale_version_dir_detection() {
        assert!(is_stale_version_dir("rumdl-v0.2.35", "rumdl-v0.2.37"));
        assert!(!is_stale_version_dir("rumdl-v0.2.37", "rumdl-v0.2.37"));
        assert!(!is_stale_version_dir("some-other-dir", "rumdl-v0.2.37"));
        // A leading "./" (as `path()` would yield) must not be classified stale;
        // the download path deliberately matches on the bare `file_name`.
        assert!(!is_stale_version_dir("./rumdl-v0.2.35", "rumdl-v0.2.37"));
    }

    #[test]
    fn extension_toml_version_matches_cargo() {
        let ext_toml = include_str!("../extension.toml");
        let ext_version = ext_toml
            .lines()
            .find_map(|line| line.strip_prefix("version = "))
            .map(|value| value.trim().trim_matches('"'))
            .expect("extension.toml declares a version");
        assert_eq!(
            ext_version,
            env!("CARGO_PKG_VERSION"),
            "extension.toml and Cargo.toml versions must match"
        );
    }

    /// Ground-truth drift check: every asset name this extension computes must
    /// actually exist in rumdl's latest release. Ignored by default (needs the
    /// network + `gh`); run in the scheduled `release-drift` workflow, or locally
    /// with `make check-assets`.
    #[test]
    #[ignore = "network: run via `make check-assets` / the release-drift workflow"]
    fn computed_asset_names_exist_in_latest_release() {
        let output = std::process::Command::new("gh")
            .args([
                "api",
                "repos/rvben/rumdl/releases/latest",
                "--jq",
                ".assets[].name",
            ])
            .output()
            .expect("failed to run `gh api`");
        assert!(
            output.status.success(),
            "gh api failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let real: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        assert!(!real.is_empty(), "latest release has no assets");

        // Derive the released version from any binary asset name.
        let version = real
            .iter()
            .find_map(|name| name.strip_prefix("rumdl-v")?.split('-').next())
            .expect("could not derive version from a release asset name");

        for &(os, arch) in SUPPORTED_PLATFORMS {
            let expected =
                asset_name(os, arch, version).expect("supported platform has an asset name");
            assert!(
                real.contains(&expected),
                "computed asset {expected:?} for {os:?}/{arch:?} is not in the latest release; \
                 rumdl's release naming may have changed"
            );
        }
    }
}
