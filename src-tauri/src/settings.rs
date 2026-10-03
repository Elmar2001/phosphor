//! PM3 runtime settings (preferred port, custom client path).
//!
//! Persisted in the `app_settings` table so they're available to the backend
//! at startup, before the frontend has loaded. Held in managed state for
//! cheap reads from `pm3::connection`.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use crate::db::Database;
use crate::error::AppError;
use crate::pm3::{binary, ports};

const SETTINGS_KEY: &str = "pm3";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pm3Settings {
    /// Port probed first during detection. `None` = auto.
    pub preferred_port: Option<String>,
    /// Absolute path to a `proxmark3` client. `None` = auto lookup.
    pub client_path: Option<String>,
}

impl Pm3Settings {
    /// Trim inputs, map empty strings to `None`, and validate both fields.
    pub fn normalized(self) -> Result<Self, AppError> {
        let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let preferred_port = clean(self.preferred_port);
        let client_path = clean(self.client_path);

        if let Some(port) = &preferred_port {
            if !ports::is_valid_port(port) {
                return Err(AppError::CommandFailed(format!("Invalid port: {}", port)));
            }
        }
        let client_path = match client_path {
            Some(raw) => Some(
                binary::validate_custom_path(&raw)
                    .map_err(AppError::CommandFailed)?
                    .display()
                    .to_string(),
            ),
            None => None,
        };

        Ok(Self {
            preferred_port,
            client_path,
        })
    }
}

pub struct SettingsState(Mutex<Pm3Settings>);

impl SettingsState {
    pub fn load(db: &Database) -> Self {
        let stored = db
            .get_setting(SETTINGS_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<Pm3Settings>(&json).ok())
            .unwrap_or_default();
        Self::new(stored)
    }

    pub fn new(settings: Pm3Settings) -> Self {
        Self(Mutex::new(settings))
    }

    pub fn get(&self) -> Pm3Settings {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set(&self, settings: Pm3Settings) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    }
}

/// Current settings, or defaults if state isn't managed (unit tests).
pub fn current<R: Runtime>(app: &AppHandle<R>) -> Pm3Settings {
    app.try_state::<SettingsState>()
        .map(|s| s.get())
        .unwrap_or_default()
}

/// The custom client path, if one is configured.
pub fn custom_client_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    current(app).client_path.map(PathBuf::from)
}

#[tauri::command]
pub fn get_pm3_settings(state: tauri::State<'_, SettingsState>) -> Pm3Settings {
    state.get()
}

#[tauri::command]
pub fn set_pm3_settings(
    settings: Pm3Settings,
    state: tauri::State<'_, SettingsState>,
    db: tauri::State<'_, Database>,
) -> Result<Pm3Settings, AppError> {
    let settings = settings.normalized()?;
    let json = serde_json::to_string(&settings)
        .map_err(|e| AppError::CommandFailed(format!("Failed to encode settings: {}", e)))?;
    db.set_setting(SETTINGS_KEY, &json)?;
    state.set(settings.clone());
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_empty_values_to_none() {
        let s = Pm3Settings {
            preferred_port: Some("  ".into()),
            client_path: Some(String::new()),
        }
        .normalized()
        .unwrap();
        assert_eq!(s, Pm3Settings::default());
    }

    #[test]
    fn trims_and_accepts_valid_port() {
        let s = Pm3Settings {
            preferred_port: Some(" COM7 ".into()),
            client_path: None,
        }
        .normalized()
        .unwrap();
        assert_eq!(s.preferred_port.as_deref(), Some("COM7"));
    }

    #[test]
    fn rejects_invalid_port() {
        let err = Pm3Settings {
            preferred_port: Some("COM7;hw reset".into()),
            client_path: None,
        }
        .normalized()
        .unwrap_err();
        assert!(err.to_string().contains("Invalid port"));
    }

    #[test]
    fn rejects_non_pm3_client_path() {
        let err = Pm3Settings {
            preferred_port: None,
            client_path: Some(if cfg!(windows) {
                r"C:\Windows\System32\cmd.exe".into()
            } else {
                "/bin/sh".into()
            }),
        }
        .normalized()
        .unwrap_err();
        assert!(err.to_string().contains("proxmark3"));
    }
}
