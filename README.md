# Phosphor

Desktop GUI for Proxmark3. Scan, clone and manage RFID/NFC cards without touching the command line.

![Windows](https://img.shields.io/badge/Windows-10%2B-blue) ![macOS](https://img.shields.io/badge/macOS-from%20source-lightgrey) ![Linux](https://img.shields.io/badge/Linux-from%20source-lightgrey) ![License](https://img.shields.io/badge/license-GPL--3.0-green) ![Version](https://img.shields.io/badge/version-1.1.0-brightgreen)

## What it does

Phosphor wraps the Proxmark3 client into a visual wizard. You plug in your Proxmark, place a card on the reader, and Phosphor handles the rest: identifying the card type, reading its data, detecting the right blank, and writing the clone. The whole process is point-and-click.

**LF (125 kHz)** cards are cloned in seconds. **HF (13.56 MHz)** cards like MIFARE Classic go through automatic key recovery (autopwn) with real-time progress, then write to a magic card.

## Supported cards

### LF (125 kHz) - 22 types

HID ProxII, EM4100, AWID, IOProx, Indala, FDX-B, HID Corporate 1000, Paradox, Keri, Viking, Visa2000, Noralsy, Presco, Jablotron, NexWatch, PAC/Stanley, SecuraKey, Gallagher, GProxII, Pyramid, NEDAP, T55x7

### HF (13.56 MHz) - 6 types

MIFARE Classic 1K/4K (with autopwn key recovery), MIFARE Ultralight, NTAG, iCLASS/PicoPass, DESFire (detection only, non-cloneable)

### Supported magic blanks

T5577 (LF), Gen1a, Gen2/CUID, Gen3, Gen4 GTU, Gen4 GDM/USCUID (HF)

## Requirements

- **Proxmark3** device (Easy, RDV4, or compatible clone)
- **Windows 10** or later (x64). macOS 10.15+ and Linux can build from source
- USB cable (data cable, not charge-only)

Proxmark3 firmware v4.20728+ recommended. The Windows installer bundles its own PM3 client, so you don't need a separate Proxmark3 installation. macOS and Linux builds use an installed [Iceman proxmark3 client](https://github.com/RfidResearchGroup/proxmark3) (Homebrew, PATH, or a path set in Settings). See [Platform setup](docs/platform-setup.md).

## Installation

1. Download `Phosphor_1.1.0_x64-setup.exe` from [Releases](../../releases)
2. Run the installer
3. Plug in your Proxmark3
4. Launch Phosphor

## Features

- **One-click cloning** for LF and HF cards
- **Auto-detection** of card type and frequency
- **MIFARE Classic autopwn** with live progress (dictionary, nested, darkside, hardnested attacks)
- **Magic card detection** identifies Gen1a through Gen4 GDM
- **Blank card data check** warns if the blank already has data written to it
- **Firmware flash** with variant picker (RDV4, RDV4+BT, Generic, Generic 256K)
- **Diagnostics** tab: checks the PM3 client starts, lists serial ports (Proxmark3 flagged by USB ID), copies a report for bug reports
- **T5577 chip detection** and password-protected chip handling
- **Sound effects** and terminal-style UI

## Building from source

```bash
# Prerequisites: Node.js 20+, Rust 1.80+, Tauri system deps for your OS

git clone https://github.com/nikitaart2000/phosphor.git
cd phosphor
npm install
npx tauri dev      # development
npx tauri build    # production build (NSIS on Windows, .app/.dmg on macOS)
```

Checks: `npm test` (frontend), `cd src-tauri && cargo test` (backend), or everything at once with `./scripts/check.sh` / `pwsh scripts/check.ps1`.

Windows builds bundle the PM3 client: put it at `src-tauri/binaries/proxmark3-x86_64-pc-windows-msvc.exe` and its DLLs in `src-tauri/pm3-libs/` (see `tauri.windows.conf.json`). Firmware images go in `src-tauri/firmware/<variant>/fullimage.elf`. Run `npm run release:check` before publishing. Details in [Releasing](docs/releasing.md).

## Tech stack

Tauri v2, React 19, TypeScript, XState v5, Rust. Dual state machine architecture: Rust backend (WizardMachine) and frontend (XState) stay in sync through Tauri commands.

## Author

Created by **nik shuv**

## License

[GPL-3.0](LICENSE) — Copyright 2025-2026 nik shuv
