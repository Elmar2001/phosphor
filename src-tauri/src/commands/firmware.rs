use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::AppError;
use crate::pm3::version::parse_detailed_hw_version;
use crate::pm3::{connection, ports};

// ---------------------------------------------------------------------------
// State — holds the running flash child process (if any) for cancellation
// ---------------------------------------------------------------------------

/// Managed state for the flash subprocess. Stored via `app.manage()`.
pub struct FlashState {
    pub child: Mutex<Option<tauri_plugin_shell::process::CommandChild>>,
    /// Set for the whole duration of `flash_firmware`, including before the
    /// child is spawned, so concurrent flash requests are rejected.
    pub active: AtomicBool,
}

impl FlashState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            active: AtomicBool::new(false),
        }
    }
}

// ---------------------------------------------------------------------------
// DTOs — serialized to frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareCheckResult {
    pub matched: bool,
    pub client_version: String,
    pub device_firmware_version: String,
    pub hardware_variant: String,
    pub firmware_path_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareProgress {
    pub phase: String,
    pub percent: u8,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const VALID_VARIANTS: &[&str] = &["rdv4", "rdv4-bt", "generic", "generic-256"];

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Check firmware version match between bundled client and device OS.
/// Called after detect_device succeeds — runs `hw version` and parses it.
#[tauri::command]
pub async fn check_firmware_version(
    app: AppHandle,
    port: String,
) -> Result<FirmwareCheckResult, AppError> {
    let output = match connection::run_command(&app, &port, "hw version").await {
        Ok(out) => out,
        Err(e) => {
            // Capabilities mismatch — PM3 client refuses to run commands because
            // the device firmware doesn't match. We can't parse hw version output,
            // but we know it's mismatched. Try to find a bundled firmware variant.
            let err_msg = e.to_string();
            if err_msg.to_lowercase().contains("capabilities") {
                return Ok(FirmwareCheckResult {
                    matched: false,
                    client_version: "bundled".to_string(),
                    device_firmware_version: "incompatible".to_string(),
                    hardware_variant: "unknown".to_string(),
                    firmware_path_exists: false,
                });
            }
            return Err(e);
        }
    };
    let info = parse_detailed_hw_version(&output);

    let fw_exists = firmware_file_exists(&app, &info.hardware_variant);

    Ok(FirmwareCheckResult {
        matched: info.versions_match,
        client_version: info.client_version,
        device_firmware_version: info.os_version,
        hardware_variant: info.hardware_variant,
        firmware_path_exists: fw_exists,
    })
}

/// Flash firmware to the connected PM3 device.
///
/// Streams progress to the frontend via Tauri events:
/// - `firmware-progress` — phase/percent updates during flash
/// - `firmware-complete` — flash finished successfully
/// - `firmware-failed` — flash failed with error
///
/// Resolves when the flash ends. The client process is held in `FlashState`
/// so `cancel_flash` can abort it; a cancelled flash emits no completion
/// event because the frontend has already left the flashing step.
#[tauri::command]
pub async fn flash_firmware(
    app: AppHandle,
    port: String,
    hardware_variant: String,
    flash_state: State<'_, FlashState>,
) -> Result<(), AppError> {
    // Validate port
    if !ports::is_valid_port(&port) {
        return Err(AppError::CommandFailed(format!("Invalid port: {}", port)));
    }

    // Validate hardware variant (prevent path traversal)
    if !VALID_VARIANTS.contains(&hardware_variant.as_str()) {
        return Err(AppError::CommandFailed(format!(
            "Invalid hardware variant: {}",
            hardware_variant
        )));
    }

    // Resolve firmware path from bundled resources
    let resource_dir = app.path().resource_dir().map_err(|e| {
        AppError::CommandFailed(format!("Failed to resolve resource dir: {}", e))
    })?;
    let fw_path = resource_dir
        .join("firmware")
        .join(&hardware_variant)
        .join("fullimage.elf");

    if !fw_path.exists() {
        return Err(AppError::CommandFailed(format!(
            "Firmware file not found: {}",
            fw_path.display()
        )));
    }

    // Strip Windows extended-length path prefix (\\?\) — PM3 can't parse it.
    // Tauri's resource_dir() returns canonicalized paths with this prefix.
    let fw_path_str = fw_path
        .to_string_lossy()
        .strip_prefix(r"\\?\")
        .unwrap_or(&fw_path.to_string_lossy())
        .to_string();

    // Reject if a flash is already running. Two clients talking to the same
    // bootloader at once can leave the device half-flashed.
    if flash_state.active.swap(true, Ordering::SeqCst) {
        return Err(AppError::CommandFailed(
            "A firmware flash is already in progress".into(),
        ));
    }
    let _active = ActiveFlashGuard(&flash_state.active);

    emit_progress(&app, "connecting", 5, "Connecting to device...");
    emit_progress(
        &app,
        "writing",
        30,
        "Flashing firmware (this may take up to 60 seconds)...",
    );

    // Milestones parsed from client output only ever move the bar forward.
    let mut percent = 30u8;
    let mut last_line = String::new();
    let result = connection::run_flash(&app, &port, &fw_path_str, &flash_state.child, |line| {
        last_line = line.to_string();
        if let Some((phase, pct, message)) = flash_milestone(line) {
            if pct > percent {
                percent = pct;
                emit_progress(&app, phase, pct, message);
            }
        }
    })
    .await;

    match result {
        Ok(_) => {
            let _ = app.emit(
                "firmware-complete",
                FirmwareProgress {
                    phase: "done".into(),
                    percent: 100,
                    message: "Firmware flash complete!".into(),
                },
            );
        }
        Err(AppError::Cancelled) => {}
        Err(e) => {
            let message = if last_line.is_empty() {
                e.to_string()
            } else {
                last_line
            };
            let _ = app.emit(
                "firmware-failed",
                FirmwareProgress {
                    phase: "error".into(),
                    percent: 0,
                    message,
                },
            );
        }
    }

    Ok(())
}

/// Cancel an in-progress firmware flash by killing the child process.
#[tauri::command]
pub async fn cancel_flash(
    app: AppHandle,
    flash_state: State<'_, FlashState>,
) -> Result<(), AppError> {
    let child = {
        let mut lock = flash_state.child.lock().map_err(|e| {
            AppError::CommandFailed(format!("Flash state lock poisoned: {}", e))
        })?;
        lock.take()
    };

    match child {
        Some(child) => {
            child.kill().map_err(|e| {
                AppError::CommandFailed(format!("Failed to kill flash process: {}", e))
            })?;
            connection::emit_output(
                &app,
                "[!] Flash aborted. If the PM3 stays in bootloader mode, flash again to recover.",
                true,
            );
            Ok(())
        }
        None => Ok(()), // No flash running — no-op
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Clears `FlashState::active` when the flash command returns, on any path.
struct ActiveFlashGuard<'a>(&'a AtomicBool);

impl Drop for ActiveFlashGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn emit_progress(app: &AppHandle, phase: &str, percent: u8, message: &str) {
    let _ = app.emit(
        "firmware-progress",
        FirmwareProgress {
            phase: phase.into(),
            percent,
            message: message.into(),
        },
    );
}

/// Map PM3 client flash output to coarse progress milestones.
fn flash_milestone(line: &str) -> Option<(&'static str, u8, &'static str)> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("all done") {
        Some(("verifying", 95, "Finishing up..."))
    } else if lower.contains("writing segments") || lower.contains("flashing") {
        Some(("writing", 60, "Writing firmware..."))
    } else if lower.contains("bootloader") || lower.contains("waiting for proxmark3") {
        Some(("bootloader", 40, "Entering bootloader..."))
    } else {
        None
    }
}

fn firmware_file_exists(app: &AppHandle, variant: &str) -> bool {
    if !VALID_VARIANTS.contains(&variant) {
        return false;
    }
    app.path()
        .resource_dir()
        .map(|dir| {
            dir.join("firmware")
                .join(variant)
                .join("fullimage.elf")
                .exists()
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_milestones_follow_client_output() {
        let pct = |line: &str| flash_milestone(line).map(|m| m.1);
        assert_eq!(pct("[+] Entering bootloader..."), Some(40));
        assert_eq!(pct("[+] Waiting for Proxmark3 to appear on COM5"), Some(40));
        assert_eq!(pct("[+] Flashing..."), Some(60));
        assert_eq!(pct("[+] Writing segments for file: fullimage.elf"), Some(60));
        assert_eq!(pct("[+] All done"), Some(95));
        assert_eq!(pct("[=] Available memory on this board: 512K bytes"), None);
    }
}
