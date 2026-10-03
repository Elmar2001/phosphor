//! Setup diagnostics: is a PM3 client installed and runnable, is a device
//! plugged in, is firmware bundled, do the user's overrides still apply.
//! Never talks to the device itself, so it is safe to run at any time.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::pm3::binary::{self, Pm3Candidate, Pm3Source};
use crate::pm3::connection::{self, ClientProbe};
use crate::pm3::ports::{self, SerialPortEntry};
use crate::settings::{self, Pm3Settings};

const FIRMWARE_VARIANTS: &[&str] = &["rdv4", "rdv4-bt", "generic", "generic-256"];

/// NTSTATUS exit codes Windows reports when a binary can't even start.
const STATUS_DLL_NOT_FOUND: i32 = 0xC000_0135_u32 as i32;
const STATUS_INVALID_IMAGE_FORMAT: i32 = 0xC000_007B_u32 as i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticCheck {
    pub id: &'static str,
    pub label: &'static str,
    pub status: CheckStatus,
    pub detail: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub path: String,
    pub source: Pm3Source,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareResource {
    pub variant: &'static str,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pm3Diagnostics {
    pub app_version: String,
    pub platform: String,
    pub overall: CheckStatus,
    pub checks: Vec<DiagnosticCheck>,
    pub client: Option<ClientInfo>,
    pub client_candidates: Vec<Pm3Candidate>,
    pub ports: Vec<SerialPortEntry>,
    pub firmware: Vec<FirmwareResource>,
    pub settings: Pm3Settings,
}

#[tauri::command]
pub async fn get_pm3_diagnostics(app: AppHandle) -> Pm3Diagnostics {
    let settings = settings::current(&app);
    let custom = settings.client_path.as_deref().map(std::path::Path::new);
    let client_candidates = binary::candidates(custom);
    let probe = connection::probe_client(&app).await;
    let enumerated = ports::list_ports();
    let firmware = firmware_resources(&app);

    let client = probe.as_ref().ok().map(|p| ClientInfo {
        path: p.path.display().to_string(),
        source: p.source,
        version: extract_client_version(&p.output),
    });

    let mut checks = vec![client_check(&probe)];
    if let Some(check) = custom_path_check(&settings, &client_candidates) {
        checks.push(check);
    }
    checks.push(ports_check(&enumerated, settings.preferred_port.as_deref()));
    checks.push(firmware_check(&firmware));

    Pm3Diagnostics {
        app_version: app.package_info().version.to_string(),
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        overall: checks
            .iter()
            .map(|c| c.status)
            .max()
            .unwrap_or(CheckStatus::Ok),
        checks,
        client,
        client_candidates,
        ports: enumerated.unwrap_or_default(),
        firmware,
        settings,
    }
}

/// Serial ports for the Settings port picker.
#[tauri::command]
pub fn list_serial_ports() -> Result<Vec<SerialPortEntry>, String> {
    ports::list_ports()
}

static CLIENT_VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\S*/v\d+\.\d+\S*").expect("bad client version regex"));

/// Pull "Iceman/master/v4.20728-..." out of `proxmark3 --version` output.
fn extract_client_version(output: &str) -> Option<String> {
    CLIENT_VERSION_RE.find(output).map(|m| {
        m.as_str()
            .trim_matches(|c| c == '(' || c == ')')
            .to_string()
    })
}

fn client_check(probe: &Result<ClientProbe, connection::ProbeError>) -> DiagnosticCheck {
    let (status, detail, hint) = match probe {
        Ok(p) => {
            let launched = p.exit_code == Some(0) || p.output.to_lowercase().contains("proxmark");
            let version =
                extract_client_version(&p.output).unwrap_or_else(|| "version not reported".into());
            if launched {
                (
                    CheckStatus::Ok,
                    format!("{} ({})", version, p.path.display()),
                    None,
                )
            } else {
                (
                    CheckStatus::Error,
                    format!(
                        "{} exited with code {} before printing anything",
                        p.path.display(),
                        p.exit_code.map_or("unknown".into(), |c| c.to_string())
                    ),
                    Some(start_failure_hint(p.exit_code)),
                )
            }
        }
        Err(connection::ProbeError::NotFound) => (
            CheckStatus::Error,
            "No proxmark3 client found".into(),
            Some(install_hint().into()),
        ),
        Err(connection::ProbeError::Spawn(e)) => {
            (CheckStatus::Error, e.clone(), Some(install_hint().into()))
        }
        Err(connection::ProbeError::TimedOut(path)) => (
            CheckStatus::Warning,
            format!("{} did not exit within 10 s", path.display()),
            Some("The client may be waiting on a terminal or blocked by security software".into()),
        ),
    };
    DiagnosticCheck {
        id: "client",
        label: "PM3 client",
        status,
        detail,
        hint,
    }
}

fn start_failure_hint(code: Option<i32>) -> String {
    match code {
        Some(STATUS_DLL_NOT_FOUND) => {
            "A DLL is missing: Qt/MinGW DLLs must sit next to proxmark3.exe".into()
        }
        Some(STATUS_INVALID_IMAGE_FORMAT) => {
            "The client was built for a different CPU architecture".into()
        }
        _ => "Run the client from a terminal to see why it fails to start".into(),
    }
}

fn install_hint() -> &'static str {
    if cfg!(target_os = "windows") {
        "Reinstall Phosphor or check antivirus quarantine; or set the client path in Settings"
    } else if cfg!(target_os = "macos") {
        "brew install rfidresearchgroup/proxmark3/proxmark3, or set the client path in Settings"
    } else {
        "Install the Iceman proxmark3 client, or set the client path in Settings"
    }
}

fn custom_path_check(
    settings: &Pm3Settings,
    candidates: &[Pm3Candidate],
) -> Option<DiagnosticCheck> {
    let path = settings.client_path.as_ref()?;
    let exists = candidates
        .iter()
        .any(|c| c.source == Pm3Source::Custom && c.exists);
    Some(DiagnosticCheck {
        id: "custom-client",
        label: "Custom client path",
        status: if exists {
            CheckStatus::Ok
        } else {
            CheckStatus::Error
        },
        detail: if exists {
            path.clone()
        } else {
            format!(
                "{} no longer exists; automatic lookup is used instead",
                path
            )
        },
        hint: (!exists).then(|| "Update or clear the path in Settings".into()),
    })
}

fn ports_check(
    enumerated: &Result<Vec<SerialPortEntry>, String>,
    preferred: Option<&str>,
) -> DiagnosticCheck {
    let (status, detail, hint) = match enumerated {
        Err(e) => (
            CheckStatus::Warning,
            format!("Could not list serial ports: {}", e),
            Some("Detection falls back to probing common port names".into()),
        ),
        Ok(list) => {
            let pm3: Vec<&str> = list
                .iter()
                .filter(|p| p.likely_pm3)
                .map(|p| p.name.as_str())
                .collect();
            let preferred_missing = preferred.filter(|port| !list.iter().any(|p| p.name == *port));
            if let Some(port) = preferred_missing {
                (
                    CheckStatus::Warning,
                    format!("Preferred port {} is not present", port),
                    Some("Plug the device in, or clear the port override in Settings".into()),
                )
            } else if !pm3.is_empty() {
                (
                    CheckStatus::Ok,
                    format!("Proxmark3 USB device on {}", pm3.join(", ")),
                    None,
                )
            } else if list.is_empty() {
                (
                    CheckStatus::Warning,
                    "No serial ports found".into(),
                    Some(no_device_hint().into()),
                )
            } else {
                (
                    CheckStatus::Warning,
                    format!(
                        "{} serial port(s), none identify as a Proxmark3",
                        list.len()
                    ),
                    Some(
                        "Detection still probes them; Bluetooth and some clones don't report USB IDs"
                            .into(),
                    ),
                )
            }
        }
    };
    DiagnosticCheck {
        id: "ports",
        label: "Serial ports",
        status,
        detail,
        hint,
    }
}

fn no_device_hint() -> &'static str {
    if cfg!(target_os = "windows") {
        "Connect the Proxmark3 with a data-capable cable and check Device Manager > Ports"
    } else if cfg!(target_os = "macos") {
        "Connect the Proxmark3 with a data-capable cable and check System Information > USB"
    } else {
        "Connect the Proxmark3; check dmesg, the dialout group, and ModemManager"
    }
}

fn firmware_resources(app: &AppHandle) -> Vec<FirmwareResource> {
    let dir = app.path().resource_dir().ok();
    FIRMWARE_VARIANTS
        .iter()
        .map(|variant| FirmwareResource {
            variant,
            available: dir.as_ref().is_some_and(|d| {
                d.join("firmware")
                    .join(variant)
                    .join("fullimage.elf")
                    .is_file()
            }),
        })
        .collect()
}

fn firmware_check(firmware: &[FirmwareResource]) -> DiagnosticCheck {
    let available: Vec<&str> = firmware
        .iter()
        .filter(|f| f.available)
        .map(|f| f.variant)
        .collect();
    DiagnosticCheck {
        id: "firmware",
        label: "Bundled firmware",
        status: if available.is_empty() {
            CheckStatus::Warning
        } else {
            CheckStatus::Ok
        },
        detail: if available.is_empty() {
            "No firmware images bundled; in-app flashing is unavailable".into()
        } else {
            format!(
                "{} of {}: {}",
                available.len(),
                firmware.len(),
                available.join(", ")
            )
        },
        hint: available
            .is_empty()
            .then(|| "Flash manually: proxmark3 <port> --flash --image fullimage.elf".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pm3::ports::PortKind;

    fn port(name: &str, likely_pm3: bool) -> SerialPortEntry {
        SerialPortEntry {
            name: name.into(),
            kind: PortKind::Usb,
            vid: None,
            pid: None,
            manufacturer: None,
            product: None,
            likely_pm3,
        }
    }

    #[test]
    fn extracts_client_version() {
        let out = " [ Client ]\n  Iceman/master/v4.20728-358-ga2ba91043-suspect 2025-01-01\n";
        assert_eq!(
            extract_client_version(out).as_deref(),
            Some("Iceman/master/v4.20728-358-ga2ba91043-suspect")
        );
        assert_eq!(extract_client_version("usage: proxmark3 [options]"), None);
    }

    #[test]
    fn statuses_order_by_severity() {
        assert!(CheckStatus::Error > CheckStatus::Warning);
        assert!(CheckStatus::Warning > CheckStatus::Ok);
    }

    #[test]
    fn windows_start_failures_get_specific_hints() {
        assert!(start_failure_hint(Some(STATUS_DLL_NOT_FOUND)).contains("DLL"));
        assert!(start_failure_hint(Some(STATUS_INVALID_IMAGE_FORMAT)).contains("architecture"));
        assert!(start_failure_hint(Some(1)).contains("terminal"));
    }

    #[test]
    fn ports_check_reports_pm3_device() {
        let check = ports_check(&Ok(vec![port("COM3", false), port("COM7", true)]), None);
        assert_eq!(check.status, CheckStatus::Ok);
        assert!(check.detail.contains("COM7"));
    }

    #[test]
    fn ports_check_flags_missing_preferred_port() {
        let check = ports_check(&Ok(vec![port("COM7", true)]), Some("COM9"));
        assert_eq!(check.status, CheckStatus::Warning);
        assert!(check.detail.contains("COM9"));
    }

    #[test]
    fn ports_check_warns_without_devices() {
        assert_eq!(ports_check(&Ok(vec![]), None).status, CheckStatus::Warning);
        assert_eq!(
            ports_check(&Ok(vec![port("COM3", false)]), None).status,
            CheckStatus::Warning
        );
        assert_eq!(
            ports_check(&Err("x".into()), None).status,
            CheckStatus::Warning
        );
    }

    #[test]
    fn firmware_check_warns_when_nothing_bundled() {
        let none = [FirmwareResource {
            variant: "rdv4",
            available: false,
        }];
        assert_eq!(firmware_check(&none).status, CheckStatus::Warning);
        let some = [
            FirmwareResource {
                variant: "rdv4",
                available: true,
            },
            FirmwareResource {
                variant: "generic",
                available: false,
            },
        ];
        let check = firmware_check(&some);
        assert_eq!(check.status, CheckStatus::Ok);
        assert_eq!(check.detail, "1 of 2: rdv4");
    }

    #[test]
    fn missing_custom_client_is_an_error() {
        let settings = Pm3Settings {
            preferred_port: None,
            client_path: Some("/nope/proxmark3".into()),
        };
        let candidates = binary::candidates(Some(std::path::Path::new("/nope/proxmark3")));
        let check = custom_path_check(&settings, &candidates).unwrap();
        assert_eq!(check.status, CheckStatus::Error);
        assert!(custom_path_check(&Pm3Settings::default(), &candidates).is_none());
    }
}
