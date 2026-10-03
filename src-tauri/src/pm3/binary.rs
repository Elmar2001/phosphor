//! Locating the `proxmark3` client executable.
//!
//! Lookup order:
//! 1. User override from Settings (validated absolute path to a `proxmark3` binary)
//! 2. Bundled sidecar next to the app executable (Tauri flattens `externalBin`
//!    entries into the executable's directory, so `binaries/proxmark3-<triple>`
//!    ships as `<exe dir>/proxmark3[.exe]`)
//! 3. `proxmark3` found on `PATH`
//! 4. Well-known install locations for the current OS. These matter on macOS,
//!    where apps launched from Finder get a minimal `PATH` without Homebrew.
//!
//! Everything resolves to an absolute path before spawning, so the Tauri shell
//! scope (which only applies to frontend IPC) is not involved.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::AppError;

#[cfg(windows)]
const EXE_NAME: &str = "proxmark3.exe";
#[cfg(not(windows))]
const EXE_NAME: &str = "proxmark3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Pm3Source {
    Custom,
    Bundled,
    Path,
    KnownLocation,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pm3Candidate {
    pub source: Pm3Source,
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone)]
pub struct Pm3Binary {
    pub path: PathBuf,
    pub source: Pm3Source,
}

/// Platform-specific install locations checked after `PATH`.
pub fn known_locations() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &[
            r"C:\proxmark3\proxmark3.exe",
            r"C:\Program Files\proxmark3\proxmark3.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/opt/homebrew/bin/proxmark3",
            "/usr/local/bin/proxmark3",
            "/opt/local/bin/proxmark3",
        ]
    } else {
        &["/usr/local/bin/proxmark3", "/usr/bin/proxmark3"]
    }
}

/// Validate a user-supplied client path. Only absolute paths to an existing
/// file named `proxmark3` (or `proxmark3.exe`) are accepted, so the setting
/// can't be used to launch arbitrary programs with PM3 arguments.
pub fn validate_custom_path(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() {
        return Err("PM3 client path must be absolute".into());
    }
    let named_pm3 = path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("proxmark3"));
    let ext_ok = match path.extension().and_then(|e| e.to_str()) {
        None => true,
        Some(ext) => ext.eq_ignore_ascii_case("exe"),
    };
    if !named_pm3 || !ext_ok {
        return Err("PM3 client path must point to a file named proxmark3".into());
    }
    if !path.is_file() {
        return Err(format!("No file at {}", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .map(|m| m.permissions().mode())
            .unwrap_or(0);
        if mode & 0o111 == 0 {
            return Err(format!("{} is not executable", path.display()));
        }
    }
    Ok(path)
}

/// All lookup candidates in priority order, with existence flags. Used by
/// `resolve()` and by the diagnostics screen.
pub fn candidates(custom: Option<&Path>) -> Vec<Pm3Candidate> {
    let mut out: Vec<(Pm3Source, PathBuf)> = Vec::new();

    if let Some(custom) = custom {
        out.push((Pm3Source::Custom, custom.to_path_buf()));
    }
    if let Some(bundled) = bundled_path() {
        out.push((Pm3Source::Bundled, bundled));
    }
    if let Some(on_path) = search_path(std::env::var_os("PATH").as_deref()) {
        out.push((Pm3Source::Path, on_path));
    }
    for location in known_locations() {
        out.push((Pm3Source::KnownLocation, PathBuf::from(location)));
    }

    let mut seen: Vec<PathBuf> = Vec::new();
    out.into_iter()
        .filter(|(_, path)| {
            if seen.contains(path) {
                false
            } else {
                seen.push(path.clone());
                true
            }
        })
        .map(|(source, path)| Pm3Candidate {
            source,
            exists: path.is_file(),
            path: path.display().to_string(),
        })
        .collect()
}

/// Resolve the PM3 client to spawn. Errors start with "Failed to spawn
/// proxmark3" because `detect_device` and the device command use that prefix
/// to tell "binary missing" apart from "device missing".
pub fn resolve(custom: Option<&Path>) -> Result<Pm3Binary, AppError> {
    let all = candidates(custom);
    if let Some(found) = all.iter().find(|c| c.exists) {
        return Ok(Pm3Binary {
            path: PathBuf::from(&found.path),
            source: found.source,
        });
    }
    Err(AppError::CommandFailed(
        "Failed to spawn proxmark3: binary not found".into(),
    ))
}

fn bundled_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(EXE_NAME))
}

fn search_path(path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let path_var = path_var?;
    std::env::split_paths(path_var)
        .map(|dir| dir.join(EXE_NAME))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "phosphor-binary-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_exe(path: &Path) {
        std::fs::write(path, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn custom_path_must_be_absolute() {
        assert!(validate_custom_path("proxmark3").is_err());
        assert!(validate_custom_path("./bin/proxmark3").is_err());
    }

    #[test]
    fn custom_path_must_be_named_proxmark3() {
        let dir = temp_dir("named");
        let other = dir.join("sh");
        make_exe(&other);
        let err = validate_custom_path(other.to_str().unwrap()).unwrap_err();
        assert!(err.contains("named proxmark3"));
    }

    #[test]
    fn custom_path_must_exist() {
        let dir = temp_dir("missing");
        let missing = dir.join(EXE_NAME);
        assert!(validate_custom_path(missing.to_str().unwrap()).is_err());
    }

    #[test]
    fn custom_path_accepts_existing_client() {
        let dir = temp_dir("valid");
        let exe = dir.join(EXE_NAME);
        make_exe(&exe);
        assert_eq!(validate_custom_path(exe.to_str().unwrap()).unwrap(), exe);
    }

    #[cfg(unix)]
    #[test]
    fn custom_path_must_be_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("noexec");
        let exe = dir.join(EXE_NAME);
        std::fs::write(&exe, b"").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(validate_custom_path(exe.to_str().unwrap()).is_err());
    }

    #[test]
    fn search_path_finds_client() {
        let dir = temp_dir("path");
        let exe = dir.join(EXE_NAME);
        make_exe(&exe);
        let path_var = std::env::join_paths([dir.join("nope"), dir.clone()]).unwrap();
        assert_eq!(search_path(Some(&path_var)), Some(exe));
    }

    #[test]
    fn custom_candidate_comes_first() {
        let dir = temp_dir("order");
        let exe = dir.join(EXE_NAME);
        make_exe(&exe);
        let all = candidates(Some(&exe));
        assert_eq!(all[0].source, Pm3Source::Custom);
        assert!(all[0].exists);
        let resolved = resolve(Some(&exe)).unwrap();
        assert_eq!(resolved.source, Pm3Source::Custom);
        assert_eq!(resolved.path, exe);
    }

    #[test]
    fn candidates_include_known_locations() {
        let all = candidates(None);
        for location in known_locations() {
            assert!(all.iter().any(|c| c.path == *location));
        }
    }
}
