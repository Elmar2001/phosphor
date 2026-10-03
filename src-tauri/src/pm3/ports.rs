//! Serial port validation, enumeration, and detection ordering.
//!
//! Detection used to brute-force a fixed list (COM1..COM40, ttyACM0..5, a few
//! macOS names), spawning the PM3 client once per guess. Ports are now
//! enumerated with their USB IDs so a Proxmark3 is probed first; the legacy
//! list is kept as a tail so a device the OS enumeration misses is still found.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serialport::SerialPortType;

/// USB vendor/product IDs used by Proxmark3 firmware (Iceman/RRG and older
/// official builds). Bootloader mode reports the same IDs.
const PM3_USB_IDS: &[(u16, u16)] = &[(0x9AC4, 0x4B8F), (0x2D2D, 0x504D)];

/// Accepted port names. The port is passed as its own argv entry, but it must
/// still never start with `-` (it would be parsed as a PM3 flag) or carry
/// separators, so only well-known device shapes are allowed:
/// - Windows `COM1`..`COM999`
/// - Linux `/dev/ttyACM*`, `/dev/ttyUSB*`, `/dev/rfcomm*` (RDV4 Bluetooth add-on)
/// - macOS `/dev/tty.*` and `/dev/cu.*` (`usbmodem*`, `usbserial*`, ...)
static PORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(COM[1-9]\d{0,2}|/dev/tty(ACM|USB)\d{1,3}|/dev/rfcomm\d{1,2}|/dev/(tty|cu)\.[A-Za-z0-9][A-Za-z0-9._-]{0,63})$",
    )
    .expect("bad port regex")
});

pub fn is_valid_port(port: &str) -> bool {
    PORT_RE.is_match(port)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortKind {
    Usb,
    Bluetooth,
    Pci,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialPortEntry {
    pub name: String,
    pub kind: PortKind,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub likely_pm3: bool,
}

/// Enumerate serial ports present on this machine. macOS exposes every device
/// twice (`/dev/tty.X` and `/dev/cu.X`); only the `tty.` twin is kept, matching
/// the PM3 documentation.
pub fn list_ports() -> Result<Vec<SerialPortEntry>, String> {
    // serialport's sysfs fallback panics if /sys/class/tty is missing (some
    // sandboxes); treat that like any other enumeration failure.
    let ports = std::panic::catch_unwind(serialport::available_ports)
        .map_err(|_| "serial port enumeration panicked".to_string())?
        .map_err(|e| e.to_string())?;

    let mut entries: Vec<SerialPortEntry> = ports
        .into_iter()
        .map(|p| {
            let (kind, vid, pid, manufacturer, product) = match p.port_type {
                SerialPortType::UsbPort(usb) => (
                    PortKind::Usb,
                    Some(usb.vid),
                    Some(usb.pid),
                    usb.manufacturer,
                    usb.product,
                ),
                SerialPortType::BluetoothPort => (PortKind::Bluetooth, None, None, None, None),
                SerialPortType::PciPort => (PortKind::Pci, None, None, None, None),
                SerialPortType::Unknown => (PortKind::Unknown, None, None, None, None),
            };
            let likely_pm3 = looks_like_pm3(
                &p.port_name,
                vid,
                pid,
                manufacturer.as_deref(),
                product.as_deref(),
            );
            SerialPortEntry {
                name: p.port_name,
                kind,
                vid,
                pid,
                manufacturer,
                product,
                likely_pm3,
            }
        })
        .collect();

    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    entries.retain(|e| match e.name.strip_prefix("/dev/cu.") {
        Some(rest) => !names.contains(&format!("/dev/tty.{}", rest)),
        None => true,
    });
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn looks_like_pm3(
    name: &str,
    vid: Option<u16>,
    pid: Option<u16>,
    manufacturer: Option<&str>,
    product: Option<&str>,
) -> bool {
    if let (Some(vid), Some(pid)) = (vid, pid) {
        if PM3_USB_IDS.contains(&(vid, pid)) {
            return true;
        }
    }
    let mentions_pm3 = |s: &str| {
        let s = s.to_ascii_lowercase();
        s.contains("proxmark") || s.contains("iceman")
    };
    manufacturer.is_some_and(mentions_pm3)
        || product.is_some_and(mentions_pm3)
        || mentions_pm3(name)
}

/// The fixed guesses detection relied on before enumeration existed.
pub fn fallback_candidates() -> Vec<String> {
    let mut ports = Vec::new();
    if cfg!(target_os = "windows") {
        // Extend to 40 to cover USB hub reassignment
        for i in 1..=40 {
            ports.push(format!("COM{}", i));
        }
    } else if cfg!(target_os = "macos") {
        for suffix in ["iceman1", "14101", "14201", "14301", "1", "2", "3"] {
            ports.push(format!("/dev/tty.usbmodem{}", suffix));
        }
    } else {
        for i in 0..=5 {
            ports.push(format!("/dev/ttyACM{}", i));
            ports.push(format!("/dev/ttyUSB{}", i));
        }
    }
    ports
}

/// Order in which detection probes ports: the user's preferred port, then
/// enumerated ports that look like a PM3, other USB serial ports, Bluetooth and
/// unknown ports, and finally the legacy guesses. On-board (PCI) UARTs are
/// skipped. Invalid names are dropped and duplicates removed.
pub fn detection_order(
    preferred: Option<&str>,
    enumerated: &Result<Vec<SerialPortEntry>, String>,
    fallback: &[String],
) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if is_valid_port(name) && !order.iter().any(|p| p == name) {
            order.push(name.to_string());
        }
    };

    if let Some(port) = preferred {
        push(port);
    }

    if let Ok(entries) = enumerated {
        let rank = |e: &SerialPortEntry| match (e.likely_pm3, e.kind) {
            (true, _) => 0,
            (false, PortKind::Usb) => 1,
            (false, PortKind::Bluetooth) => 2,
            (false, PortKind::Unknown) => 3,
            (false, PortKind::Pci) => 4,
        };
        let mut ranked: Vec<&SerialPortEntry> = entries.iter().filter(|e| rank(e) < 4).collect();
        ranked.sort_by_key(|e| rank(e));
        for entry in ranked {
            push(&entry.name);
        }
    }

    for port in fallback {
        push(port);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: PortKind, likely_pm3: bool) -> SerialPortEntry {
        SerialPortEntry {
            name: name.to_string(),
            kind,
            vid: None,
            pid: None,
            manufacturer: None,
            product: None,
            likely_pm3,
        }
    }

    #[test]
    fn accepts_known_port_shapes() {
        for port in [
            "COM3",
            "COM40",
            "COM256",
            "/dev/ttyACM0",
            "/dev/ttyUSB12",
            "/dev/rfcomm0",
            "/dev/tty.usbmodemiceman1",
            "/dev/cu.usbmodem14101",
            "/dev/tty.usbserial-A10K1234",
            "/dev/cu.wchusbserial1410",
            "/dev/tty.SLAB_USBtoUART",
        ] {
            assert!(is_valid_port(port), "{} should be valid", port);
        }
    }

    #[test]
    fn rejects_injection_and_unknown_shapes() {
        for port in [
            "",
            "COM0",
            "COM3;lf t55xx wipe",
            "COM3 --flash",
            "--flash",
            "-c",
            "/dev/ttyACM0;hw reset",
            "/dev/sda",
            "/dev/tty.",
            "/dev/tty.-flag",
            "/dev/tty.usbmodem 1",
            "/dev/../etc/passwd",
            "/dev/ttyS0",
        ] {
            assert!(!is_valid_port(port), "{:?} should be rejected", port);
        }
    }

    #[test]
    fn pm3_usb_ids_are_recognized() {
        assert!(looks_like_pm3(
            "COM7",
            Some(0x9AC4),
            Some(0x4B8F),
            None,
            None
        ));
        assert!(looks_like_pm3(
            "COM7",
            Some(0x2D2D),
            Some(0x504D),
            None,
            None
        ));
        assert!(looks_like_pm3(
            "/dev/tty.usbmodemiceman1",
            None,
            None,
            None,
            None
        ));
        assert!(looks_like_pm3(
            "/dev/ttyACM0",
            Some(1),
            Some(2),
            None,
            Some("proxmark3")
        ));
        assert!(!looks_like_pm3(
            "COM4",
            Some(0x1A86),
            Some(0x7523),
            Some("wch.cn"),
            None
        ));
    }

    #[test]
    fn detection_prefers_user_port_then_pm3_then_usb() {
        let enumerated = Ok(vec![
            entry("COM1", PortKind::Pci, false),
            entry("COM4", PortKind::Usb, false),
            entry("COM9", PortKind::Bluetooth, false),
            entry("COM7", PortKind::Usb, true),
        ]);
        let order = detection_order(Some("COM12"), &enumerated, &["COM1".into(), "COM2".into()]);
        assert_eq!(order, vec!["COM12", "COM7", "COM4", "COM9", "COM1", "COM2"]);
    }

    #[test]
    fn detection_skips_invalid_and_duplicate_ports() {
        let enumerated = Ok(vec![
            entry("/dev/ttyS0", PortKind::Unknown, false),
            entry("/dev/ttyACM0", PortKind::Usb, true),
        ]);
        let order = detection_order(
            Some("not a port"),
            &enumerated,
            &["/dev/ttyACM0".into(), "/dev/ttyACM1".into()],
        );
        assert_eq!(order, vec!["/dev/ttyACM0", "/dev/ttyACM1"]);
    }

    #[test]
    fn detection_falls_back_when_enumeration_fails() {
        let order = detection_order(None, &Err("boom".into()), &["COM1".into(), "COM2".into()]);
        assert_eq!(order, vec!["COM1", "COM2"]);
    }

    #[test]
    fn fallback_candidates_are_valid_ports() {
        for port in fallback_candidates() {
            assert!(is_valid_port(&port), "{} should be valid", port);
        }
    }
}
