use std::{
    ffi::OsStr,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio::process::Command;

pub fn resolve_codex_binary() -> Result<PathBuf, String> {
    find_binary(
        std::env::var_os("CODEX_CLI_PATH").map(PathBuf::from),
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
    )
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn find_binary(
    explicit: Option<PathBuf>,
    search_path: Option<&OsStr>,
    home: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return if executable(&path) {
            Ok(path)
        } else {
            Err("CODEX_CLI_PATH must point to an executable Codex CLI file".into())
        };
    }
    let mut candidates: Vec<PathBuf> = search_path
        .map(|value| {
            std::env::split_paths(value)
                .filter(|path| path.is_absolute())
                .map(|path| path.join("codex"))
                .collect()
        })
        .unwrap_or_default();
    if let Some(home) = home {
        candidates.extend([
            home.join(".local/bin/codex"),
            home.join(".npm-global/bin/codex"),
            home.join(".volta/bin/codex"),
            home.join(".cargo/bin/codex"),
            home.join(".asdf/shims/codex"),
        ]);
        // Desktop launchers do not source .bashrc, where nvm normally sets PATH.
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions: Vec<_> = entries.filter_map(Result::ok).collect();
            versions.sort_by_key(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .trim_start_matches('v')
                    .split('.')
                    .map(|part| part.parse::<u32>().unwrap_or(0))
                    .collect::<Vec<_>>()
            });
            candidates.extend(
                versions
                    .into_iter()
                    .rev()
                    .map(|entry| entry.path().join("bin/codex")),
            );
        }
    }
    candidates.extend([
        PathBuf::from("/usr/local/bin/codex"),
        PathBuf::from("/usr/bin/codex"),
        PathBuf::from("/snap/bin/codex"),
        PathBuf::from("/usr/lib/codex/resources/codex"),
        PathBuf::from("/usr/lib/chatgpt/resources/codex"),
        PathBuf::from("/opt/Codex/resources/codex"),
    ]);
    candidates.into_iter().find(|path| executable(path)).ok_or_else(|| {
        "Codex CLI was not found. Install and sign in to Codex, or set CODEX_CLI_PATH to its executable.".into()
    })
}

pub fn configure_command(command: &mut Command) {
    // npm's launcher uses /usr/bin/env node. Include its sibling Node binary
    // for all account probes, app-server connections and scheduled sessions.
    let binary = PathBuf::from(command.as_std().get_program());
    let app_dir = std::env::var_os("APPDIR").map(PathBuf::from);
    let mut directories = Vec::new();
    if let Some(parent) = binary.parent().filter(|path| !path.as_os_str().is_empty()) {
        directories.push(parent.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PATH") {
        directories.extend(std::env::split_paths(&path).filter(|directory| {
            app_dir
                .as_ref()
                .is_none_or(|bundle| !directory.starts_with(bundle))
        }));
    }
    directories.extend([
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);
    if let Ok(path) = std::env::join_paths(directories) {
        command.env("PATH", path);
    }
    if let Some(bundle) = app_dir {
        // AppRun injects its bundled Python and shared-library paths. Those
        // belong to QuotaMate, not to the user's external Codex executable.
        for key in [
            "LD_LIBRARY_PATH",
            "PYTHONPATH",
            "GI_TYPELIB_PATH",
            "GIO_EXTRA_MODULES",
        ] {
            if let Some(value) = std::env::var_os(key) {
                let paths: Vec<_> = std::env::split_paths(&value)
                    .filter(|path| !path.starts_with(&bundle) && !path.as_os_str().is_empty())
                    .collect();
                if paths.is_empty() {
                    command.env_remove(key);
                } else if let Ok(value) = std::env::join_paths(paths) {
                    command.env(key, value);
                }
            }
        }
        for key in [
            "PYTHONHOME",
            "GTK_PATH",
            "GTK_EXE_PREFIX",
            "GTK_DATA_PREFIX",
            "GSETTINGS_SCHEMA_DIR",
            "GTK_IM_MODULE_FILE",
            "GDK_PIXBUF_MODULE_FILE",
        ] {
            if std::env::var_os(key).is_some_and(|value| Path::new(&value).starts_with(&bundle)) {
                command.env_remove(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("quotamate-cli-{}", uuid::Uuid::new_v4())))
        }
        fn file(&self, relative: &str, mode: u32) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn explicit_binary_requires_execute_permission() {
        let fixture = Fixture::new();
        let path = fixture.file("custom/codex", 0o644);
        assert!(find_binary(Some(path.clone()), None, None).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(find_binary(Some(path.clone()), None, None).unwrap(), path);
    }

    #[test]
    fn desktop_launch_finds_newest_nvm_cli_and_respects_path() {
        let fixture = Fixture::new();
        fixture.file(".nvm/versions/node/v9.0.0/bin/codex", 0o755);
        let newest = fixture.file(".nvm/versions/node/v24.1.0/bin/codex", 0o755);
        assert_eq!(
            find_binary(None, Some(OsStr::new("")), Some(&fixture.0)).unwrap(),
            newest
        );
        let preferred = fixture.file("preferred/codex", 0o755);
        assert_eq!(
            find_binary(
                None,
                preferred.parent().map(Path::as_os_str),
                Some(&fixture.0)
            )
            .unwrap(),
            preferred
        );
    }

    #[tokio::test]
    async fn npm_launcher_can_find_sibling_node() {
        let fixture = Fixture::new();
        std::fs::create_dir_all(fixture.0.join("bin")).unwrap();
        let binary = fixture.0.join("bin/codex");
        // Use existing executables so concurrent process tests cannot inherit
        // a newly written script descriptor and cause ETXTBSY at exec time.
        std::os::unix::fs::symlink("/usr/bin/env", &binary).unwrap();
        std::os::unix::fs::symlink("/bin/true", fixture.0.join("bin/quotamate-test-node")).unwrap();
        let mut command = Command::new(&binary);
        command.arg("quotamate-test-node");
        configure_command(&mut command);
        assert!(command.status().await.unwrap().success());
    }
}
