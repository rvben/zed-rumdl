use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use zed_extension_api::{self as zed, LanguageServerId, Result, settings::LspSettings};

/// The Rust target triple rumdl uses in its release asset names for this
/// platform, or `None` if rumdl does not publish a binary for it. Linux uses
/// the statically linked musl builds, which run on every distribution; the
/// glibc builds fail to start on musl systems and on older glibc versions, and
/// an extension cannot tell which libc its host has.
fn target_triple(os: zed::Os, arch: zed::Architecture) -> Option<&'static str> {
    Some(match (os, arch) {
        (zed::Os::Mac, zed::Architecture::Aarch64) => "aarch64-apple-darwin",
        (zed::Os::Mac, zed::Architecture::X8664) => "x86_64-apple-darwin",
        (zed::Os::Linux, zed::Architecture::Aarch64) => "aarch64-unknown-linux-musl",
        (zed::Os::Linux, zed::Architecture::X8664) => "x86_64-unknown-linux-musl",
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

/// The file name of the rumdl executable inside a release archive.
fn binary_name(os: zed::Os) -> &'static str {
    match os {
        zed::Os::Windows => "rumdl.exe",
        _ => "rumdl",
    }
}

/// The path to the extracted rumdl binary, relative to the extension work dir.
fn binary_relative_path(version: &str, os: zed::Os) -> String {
    format!("{}/{}", version_dir(version), binary_name(os))
}

/// Whether `binary_path` (relative to `work_dir`) is an installed rumdl binary.
fn is_installed(work_dir: &Path, binary_path: &str) -> bool {
    fs::metadata(work_dir.join(binary_path)).is_ok_and(|stat| stat.is_file())
}

/// The directory a release is downloaded and extracted into before it is moved
/// to its version directory, relative to the extension work dir. The name must
/// not start with `rumdl-`, which marks installed versions.
const STAGING_DIR: &str = "download-staging";

/// Remove `dir` and its contents; a directory that does not exist is fine.
fn remove_dir_if_present(dir: &Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("failed to remove {}: {err}", dir.display())),
    }
}

/// Install rumdl `version` under `work_dir` so that its version directory only
/// ever exists complete. `fetch` downloads and extracts the release into the
/// staging directory it is given (relative to the work dir), and a single
/// rename then publishes it. A download that fails or is cut off, even by Zed
/// quitting mid-extraction, leaves no version directory behind, so the binary
/// existing means it is whole and the next start downloads again otherwise.
fn install_release(
    work_dir: &Path,
    version: &str,
    os: zed::Os,
    fetch: impl FnOnce(&str) -> Result<()>,
) -> Result<()> {
    let staging = work_dir.join(STAGING_DIR);
    let installed = work_dir.join(version_dir(version));

    remove_dir_if_present(&staging)?;
    let fetched = fetch(STAGING_DIR).and_then(|()| {
        let binary = binary_name(os);
        if staging.join(binary).is_file() {
            Ok(())
        } else {
            Err(format!("the downloaded release contains no {binary}"))
        }
    });
    if let Err(err) = fetched {
        fs::remove_dir_all(&staging).ok();
        return Err(err);
    }

    // A version directory without its binary is a leftover that would block
    // the rename.
    remove_dir_if_present(&installed)?;
    fs::rename(&staging, &installed)
        .map_err(|err| format!("failed to move the download into place: {err}"))
}

/// Remove every downloaded rumdl version under `work_dir` except
/// `current_version_dir`. Failures are ignored: a leftover directory costs
/// disk space only, and the next download retries the cleanup.
fn remove_stale_versions(work_dir: &Path, current_version_dir: &str) {
    let Ok(entries) = fs::read_dir(work_dir) else {
        return;
    };
    for entry in entries.flatten() {
        // Match on the bare entry name: `path` carries the work dir prefix.
        if is_stale_version_dir(&entry.file_name().to_string_lossy(), current_version_dir) {
            fs::remove_dir_all(entry.path()).ok();
        }
    }
}

/// Whether `entry_name` is a leftover rumdl download directory from a version
/// other than the one currently in use, and so safe to remove.
fn is_stale_version_dir(entry_name: &str, current_version_dir: &str) -> bool {
    entry_name.starts_with("rumdl-") && entry_name != current_version_dir
}

/// Numeric sort key for a version like "v0.2.35", so that 0.2.10 orders after
/// 0.2.9. Non-numeric segments sort as 0.
fn version_sort_key(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split('.')
        .map(|segment| segment.parse().unwrap_or(0))
        .collect()
}

/// The binaries of previously downloaded rumdl versions among the work-dir
/// entries, newest first, used as fallback candidates when GitHub cannot be
/// reached to determine or download the latest release. Every version is a
/// candidate because an interrupted download can leave the newest directory
/// without a binary.
fn downloaded_binaries_newest_first(entries: &[String], os: zed::Os) -> Vec<String> {
    let mut versions: Vec<(Vec<u64>, &str)> = entries
        .iter()
        .filter_map(|name| {
            let version = name.strip_prefix("rumdl-")?;
            Some((version_sort_key(version), version))
        })
        .collect();
    versions.sort_by(|a, b| b.cmp(a));
    versions
        .into_iter()
        .map(|(_, version)| binary_relative_path(version, os))
        .collect()
}

/// Default arguments used to launch rumdl's language server. rumdl speaks LSP
/// over stdio by default (`--stdio` exists only as a hidden compatibility
/// flag), so the canonical invocation is plain `rumdl server`.
fn server_args() -> Vec<String> {
    vec!["server".to_string()]
}

/// How the rumdl binary being launched was located. System binaries (an
/// explicit settings path or a PATH lookup) inherit the worktree's shell
/// environment; downloaded binaries run in a clean environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinarySource {
    SettingsPath,
    PathEnv,
    Downloaded,
}

/// The system binary to use, if any: an explicit `lsp.rumdl.binary.path` from
/// settings wins over `rumdl` on PATH, and an empty configured path is
/// ignored. `None` means a downloaded release is needed. Settings and PATH are
/// consulted on every resolution so configuration changes apply without
/// restarting Zed; only download results are ever cached.
fn choose_binary(
    settings_path: Option<String>,
    path_env: Option<String>,
) -> Option<(String, BinarySource)> {
    if let Some(path) = settings_path.filter(|path| !path.is_empty()) {
        return Some((path, BinarySource::SettingsPath));
    }
    path_env.map(|path| (path, BinarySource::PathEnv))
}

/// The command that launches rumdl: user-configured `binary.arguments` replace
/// the default `server` invocation, and `binary.env` entries override
/// same-named shell variables. `shell_env` is a closure so the host call is
/// only made for system binaries; downloaded binaries run in a clean
/// environment. Configured entries are appended in sorted order so the
/// resulting command is deterministic.
fn build_command(
    path: String,
    source: BinarySource,
    settings_args: Option<Vec<String>>,
    settings_env: Option<HashMap<String, String>>,
    shell_env: impl FnOnce() -> Vec<(String, String)>,
) -> zed::Command {
    let mut env = match source {
        BinarySource::Downloaded => Vec::new(),
        BinarySource::SettingsPath | BinarySource::PathEnv => shell_env(),
    };
    if let Some(overrides) = settings_env {
        let mut overrides: Vec<(String, String)> = overrides.into_iter().collect();
        overrides.sort();
        env.retain(|(name, _)| !overrides.iter().any(|(configured, _)| configured == name));
        env.extend(overrides);
    }

    zed::Command {
        command: path,
        args: settings_args.unwrap_or_else(server_args),
        env,
    }
}

/// The configuration to send in rumdl's `initialize` request: an explicit
/// `lsp.rumdl.initialization_options` wins; otherwise `lsp.rumdl.settings` is
/// passed, which rumdl deserializes as its LSP configuration.
fn initialization_options_from(lsp_settings: LspSettings) -> Option<zed::serde_json::Value> {
    lsp_settings
        .initialization_options
        .or(lsp_settings.settings)
}

/// The newest previously downloaded rumdl binary that still exists on disk
/// under `work_dir`, if any, as a path relative to it.
fn existing_download(work_dir: &Path, os: zed::Os) -> Option<String> {
    let entries: Vec<String> = fs::read_dir(work_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    downloaded_binaries_newest_first(&entries, os)
        .into_iter()
        .find(|path| is_installed(work_dir, path))
}

/// The extension work dir. Zed runs the extension with it as the current
/// directory and resolves the paths given to its download API against it.
fn work_dir() -> &'static Path {
    Path::new(".")
}

struct RumdlExtension {
    /// Relative path of the downloaded binary resolved earlier in this
    /// session, kept to avoid re-querying GitHub on every server start.
    cached_download: Option<String>,
}

impl RumdlExtension {
    /// The downloaded rumdl binary to launch: the one resolved earlier this
    /// session if it still exists, else the latest GitHub release, else (when
    /// GitHub cannot be reached) the newest previously downloaded version.
    fn downloaded_binary(&mut self, language_server_id: &LanguageServerId) -> Result<String> {
        if let Some(path) = &self.cached_download
            && is_installed(work_dir(), path)
        {
            return Ok(path.clone());
        }

        let path = match self.download_latest(language_server_id) {
            Ok(path) => path,
            // Offline fallback: prefer a stale rumdl over no language server,
            // but surface the original error when there is nothing to run.
            Err(err) => existing_download(work_dir(), zed::current_platform().0).ok_or(err)?,
        };
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::None,
        );
        self.cached_download = Some(path.clone());
        Ok(path)
    }

    /// Download the latest rumdl release for this platform (a no-op if that
    /// version is already present) and remove older downloaded versions.
    fn download_latest(&mut self, language_server_id: &LanguageServerId) -> Result<String> {
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

        if !is_installed(work_dir(), &binary_path) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            install_release(work_dir(), &release.version, platform, |staging_dir| {
                zed::download_file(
                    &asset.download_url,
                    staging_dir,
                    match platform {
                        zed::Os::Windows => zed::DownloadedFileType::Zip,
                        _ => zed::DownloadedFileType::GzipTar,
                    },
                )
                .map_err(|e| format!("failed to download file: {e}"))?;
                zed::make_file_executable(&format!("{staging_dir}/{}", binary_name(platform)))
            })?;

            remove_stale_versions(work_dir(), &version_dir);
        }

        Ok(binary_path)
    }
}

impl zed::Extension for RumdlExtension {
    fn new() -> Self {
        Self {
            cached_download: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        let (settings_path, settings_args, settings_env) = match lsp_settings.binary {
            Some(binary) => (binary.path, binary.arguments, binary.env),
            None => (None, None, None),
        };

        let (path, source) = match choose_binary(settings_path, worktree.which("rumdl")) {
            Some(system) => system,
            None => (
                self.downloaded_binary(language_server_id)?,
                BinarySource::Downloaded,
            ),
        };

        Ok(build_command(
            path,
            source,
            settings_args,
            settings_env,
            || worktree.shell_env(),
        ))
    }

    fn language_server_initialization_options(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        // rumdl reads its config from `initialize` options and does not pull
        // `workspace/configuration`, so this is the path that carries user
        // settings to the server at startup.
        let lsp_settings = LspSettings::for_worktree(server_id.as_ref(), worktree)?;
        Ok(initialization_options_from(lsp_settings))
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        // `lsp.rumdl.settings` is the `workspace/didChangeConfiguration`
        // payload: Zed sends one right after `initialize` and again whenever
        // the settings change.
        Ok(LspSettings::for_worktree(server_id.as_ref(), worktree)?.settings)
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
    fn settings_binary_path_beats_path_lookup() {
        // An explicit `lsp.rumdl.binary.path` must win over rumdl on PATH, and
        // must be honored on every resolution (it is never shadowed by a cache).
        assert_eq!(
            choose_binary(
                Some("/opt/rumdl".to_string()),
                Some("/usr/bin/rumdl".to_string())
            ),
            Some(("/opt/rumdl".to_string(), BinarySource::SettingsPath)),
        );
        // An empty configured path is ignored, not launched.
        assert_eq!(
            choose_binary(Some(String::new()), Some("/usr/bin/rumdl".to_string())),
            Some(("/usr/bin/rumdl".to_string(), BinarySource::PathEnv)),
        );
        // Nothing configured and nothing on PATH means a download is needed.
        assert_eq!(choose_binary(None, None), None);
    }

    #[test]
    fn user_arguments_replace_default_server_args() {
        // `lsp.rumdl.binary.arguments` must reach the launched command, e.g. to
        // pass rumdl's global `--config` flag or `-v`.
        let command = build_command(
            "/opt/rumdl".to_string(),
            BinarySource::SettingsPath,
            Some(vec![
                "server".to_string(),
                "--config".to_string(),
                "custom.toml".to_string(),
            ]),
            None,
            Vec::new,
        );
        assert_eq!(command.command, "/opt/rumdl");
        assert_eq!(command.args, vec!["server", "--config", "custom.toml"]);

        // Without configured arguments the default `server` invocation is used.
        let command = build_command(
            "rumdl-v0.2.35/rumdl".to_string(),
            BinarySource::Downloaded,
            None,
            None,
            Vec::new,
        );
        assert_eq!(command.args, server_args());
    }

    #[test]
    fn command_env_combines_shell_env_and_user_overrides() {
        let shell_env = || {
            vec![
                ("PATH".to_string(), "/usr/bin".to_string()),
                ("RUMDL_LOG".to_string(), "info".to_string()),
            ]
        };
        let user_env = HashMap::from([("RUMDL_LOG".to_string(), "debug".to_string())]);

        // A system binary inherits the shell environment, with user-configured
        // `binary.env` entries overriding same-named variables.
        let command = build_command(
            "/usr/bin/rumdl".to_string(),
            BinarySource::PathEnv,
            None,
            Some(user_env.clone()),
            shell_env,
        );
        assert_eq!(
            command.env,
            vec![
                ("PATH".to_string(), "/usr/bin".to_string()),
                ("RUMDL_LOG".to_string(), "debug".to_string()),
            ]
        );

        // A downloaded binary runs clean: no shell environment, but explicit
        // user-configured variables still apply.
        let command = build_command(
            "rumdl-v0.2.35/rumdl".to_string(),
            BinarySource::Downloaded,
            None,
            Some(user_env),
            || panic!("shell env must not be fetched for downloaded binaries"),
        );
        assert_eq!(
            command.env,
            vec![("RUMDL_LOG".to_string(), "debug".to_string())]
        );
    }

    #[test]
    fn offline_fallback_offers_all_downloads_newest_first() {
        // When GitHub is unreachable, previously downloaded binaries must be
        // offered newest first, with versions compared numerically (0.2.10
        // beats 0.2.9). Every version is a candidate, not just the newest: an
        // interrupted download can leave the newest dir without a binary, and
        // an older complete download must still be usable.
        let entries = [
            "rumdl-v0.2.9".to_string(),
            "rumdl-v0.2.10".to_string(),
            "some-other-dir".to_string(),
        ];
        assert_eq!(
            downloaded_binaries_newest_first(&entries, Os::Linux),
            vec!["rumdl-v0.2.10/rumdl", "rumdl-v0.2.9/rumdl"]
        );
        assert_eq!(
            downloaded_binaries_newest_first(&entries, Os::Windows),
            vec!["rumdl-v0.2.10/rumdl.exe", "rumdl-v0.2.9/rumdl.exe"]
        );
        // No prior download means no fallback candidates.
        assert!(downloaded_binaries_newest_first(&["other".to_string()], Os::Mac).is_empty());
        assert!(downloaded_binaries_newest_first(&[], Os::Mac).is_empty());
    }

    /// A `fetch` that extracts a release holding `contents` as the rumdl
    /// binary into the staging directory under `work_dir`.
    fn fetch_release<'a>(
        work_dir: &'a Path,
        contents: &'a str,
    ) -> impl FnOnce(&str) -> Result<()> + 'a {
        move |staging_dir| {
            let staging = work_dir.join(staging_dir);
            fs::create_dir_all(&staging).map_err(|err| err.to_string())?;
            fs::write(staging.join("rumdl"), contents).map_err(|err| err.to_string())
        }
    }

    #[test]
    fn install_publishes_the_binary_at_its_versioned_path() {
        let work_dir = tempfile::tempdir().unwrap();
        let binary_path = binary_relative_path("v0.2.35", Os::Linux);

        install_release(
            work_dir.path(),
            "v0.2.35",
            Os::Linux,
            fetch_release(work_dir.path(), "binary"),
        )
        .unwrap();

        assert!(is_installed(work_dir.path(), &binary_path));
        assert_eq!(
            fs::read_to_string(work_dir.path().join(&binary_path)).unwrap(),
            "binary"
        );
        assert!(!work_dir.path().join(STAGING_DIR).exists());
    }

    #[test]
    fn interrupted_download_is_not_mistaken_for_an_install() {
        // A download that dies after part of the binary reached the disk must
        // leave nothing that the next start takes for a complete install,
        // or the truncated binary would be launched forever.
        let work_dir = tempfile::tempdir().unwrap();
        let binary_path = binary_relative_path("v0.2.35", Os::Linux);

        let result = install_release(work_dir.path(), "v0.2.35", Os::Linux, |staging_dir| {
            fetch_release(work_dir.path(), "trunc")(staging_dir)?;
            Err("connection reset".to_string())
        });

        assert_eq!(result, Err("connection reset".to_string()));
        assert!(!is_installed(work_dir.path(), &binary_path));
        assert!(!work_dir.path().join(version_dir("v0.2.35")).exists());
        assert!(!work_dir.path().join(STAGING_DIR).exists());

        // The next attempt downloads again and installs the whole binary.
        install_release(
            work_dir.path(),
            "v0.2.35",
            Os::Linux,
            fetch_release(work_dir.path(), "binary"),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(work_dir.path().join(&binary_path)).unwrap(),
            "binary"
        );
    }

    #[test]
    fn install_recovers_from_leftovers_of_a_killed_download() {
        // Zed quitting mid-extraction runs no cleanup: the staging directory
        // keeps its partial contents, and a version directory without a binary
        // may exist. Neither may block or leak into the next install.
        let work_dir = tempfile::tempdir().unwrap();
        let staging = work_dir.path().join(STAGING_DIR);
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("partial-file"), "trunc").unwrap();
        let installed = work_dir.path().join(version_dir("v0.2.35"));
        fs::create_dir_all(&installed).unwrap();
        fs::write(installed.join("leftover"), "old").unwrap();

        install_release(
            work_dir.path(),
            "v0.2.35",
            Os::Linux,
            fetch_release(work_dir.path(), "binary"),
        )
        .unwrap();

        assert!(is_installed(
            work_dir.path(),
            &binary_relative_path("v0.2.35", Os::Linux)
        ));
        assert!(!installed.join("partial-file").exists());
        assert!(!installed.join("leftover").exists());
    }

    #[test]
    fn release_without_the_binary_is_rejected() {
        // The Windows archive holds `rumdl.exe`; an archive that extracts
        // without the expected executable is a failed install, not a version
        // directory that can never be launched.
        let work_dir = tempfile::tempdir().unwrap();

        let result = install_release(
            work_dir.path(),
            "v0.2.35",
            Os::Windows,
            fetch_release(work_dir.path(), "not an exe"),
        );

        assert_eq!(
            result,
            Err("the downloaded release contains no rumdl.exe".to_string())
        );
        assert!(!work_dir.path().join(version_dir("v0.2.35")).exists());
        assert!(!work_dir.path().join(STAGING_DIR).exists());
    }

    #[test]
    fn stale_versions_are_removed_and_everything_else_kept() {
        let work_dir = tempfile::tempdir().unwrap();
        for dir in ["rumdl-v0.2.34", "rumdl-v0.2.35", "unrelated", STAGING_DIR] {
            fs::create_dir(work_dir.path().join(dir)).unwrap();
        }

        remove_stale_versions(work_dir.path(), "rumdl-v0.2.35");

        let mut remaining: Vec<String> = fs::read_dir(work_dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        remaining.sort();
        assert_eq!(remaining, [STAGING_DIR, "rumdl-v0.2.35", "unrelated"]);
    }

    #[test]
    fn offline_fallback_skips_versions_without_a_binary() {
        let work_dir = tempfile::tempdir().unwrap();
        assert_eq!(existing_download(work_dir.path(), Os::Linux), None);

        // The newest directory has no binary, so the older complete download
        // is the one to run.
        fs::create_dir(work_dir.path().join("rumdl-v0.2.10")).unwrap();
        fs::create_dir(work_dir.path().join("rumdl-v0.2.9")).unwrap();
        fs::write(work_dir.path().join("rumdl-v0.2.9/rumdl"), "binary").unwrap();
        assert_eq!(
            existing_download(work_dir.path(), Os::Linux),
            Some("rumdl-v0.2.9/rumdl".to_string())
        );

        // A Windows host looks for `rumdl.exe`, which is not there.
        assert_eq!(existing_download(work_dir.path(), Os::Windows), None);
    }

    #[test]
    fn explicit_initialization_options_win_over_settings() {
        use zed::serde_json::json;
        use zed_extension_api::settings::LspSettings;

        let explicit = LspSettings {
            binary: None,
            initialization_options: Some(json!({"enableAutoFix": true})),
            settings: Some(json!({"settings": {"lineLength": 120}})),
        };
        assert_eq!(
            initialization_options_from(explicit),
            Some(json!({"enableAutoFix": true}))
        );

        // Without an explicit `initialization_options` key, `settings` carries
        // the configuration to rumdl's `initialize` request.
        let settings_only = LspSettings {
            binary: None,
            initialization_options: None,
            settings: Some(json!({"settings": {"lineLength": 120}})),
        };
        assert_eq!(
            initialization_options_from(settings_only),
            Some(json!({"settings": {"lineLength": 120}}))
        );

        let empty = LspSettings {
            binary: None,
            initialization_options: None,
            settings: None,
        };
        assert_eq!(initialization_options_from(empty), None);
    }

    #[test]
    fn new_extension_has_default_state() {
        let ext = RumdlExtension::new();
        assert!(ext.cached_download.is_none());
    }

    #[test]
    fn server_args_are_stdio_default() {
        // rumdl serves LSP over stdio by default, so the default invocation is
        // exactly `rumdl server` with no transport flag.
        assert_eq!(server_args(), vec!["server".to_string()]);
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
                "rumdl-v0.2.35-aarch64-unknown-linux-musl.tar.gz",
            ),
            (
                Os::Linux,
                Architecture::X8664,
                "rumdl-v0.2.35-x86_64-unknown-linux-musl.tar.gz",
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
