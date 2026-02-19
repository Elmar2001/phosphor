# macOS Branch — Detailed Changes

This document describes all changes in the `macOS` branch compared to `master`. The branch adds macOS support, cross-platform documentation, and platform-specific troubleshooting.

---

## Summary of Commits

| Commit   | Description                    |
|----------|--------------------------------|
| `87d8a02` | Macos update                   |
| `6f0ee3c` | added binary, added agents      |
| `da34269` | Added agents.md                |

---

## 1. Build & Packaging

### 1.1 Tauri Bundle Configuration (`src-tauri/tauri.conf.json`)

- **`bundle.targets`**: Changed from `["nsis"]` to `"all"` — enables building for all platforms (Windows NSIS, macOS DMG/App, Linux) instead of Windows-only.
- **`bundle.category`**: Added `"Utility"` for macOS app categorization.
- **`bundle.macOS`**: New section with:
  - `minimumSystemVersion`: `"10.15"` (Catalina)
  - `signingIdentity`: `null` (unsigned builds)
  - `entitlements`: `"Entitlements.plist"` — no app sandbox (required for serial port access)
  - `hardenedRuntime`: `false` — allows spawning external binaries (shell plugin fallback)

### 1.2 Entitlements (`src-tauri/Entitlements.plist`)

- **Added**: Entitlements file with no `com.apple.security.app-sandbox` — app runs without sandbox so it can access serial ports (`/dev/cu.usbmodem*`, `/dev/tty.usbmodem*`) for Proxmark3.

### 1.3 Sidecar Binary

- **Added**: `src-tauri/binaries/proxmark3-aarch64-apple-darwin` — Proxmark3 client binary compiled for Apple Silicon (aarch64). Required for running Phosphor on M1/M2/M3 Macs.

### 1.4 Version Bump (`package-lock.json`)

- **`version`**: `0.1.0` → `1.1.0` (aligned with release version).

---

## 2. macOS DMG: Native PM3 Spawn (`connection.rs`)

When the app is launched from a DMG, the Tauri shell plugin fails to spawn external binaries (Hardened Runtime / capability restrictions). The fix:

- **`try_native_pm3_macos()`**: Uses `tokio::process::Command` to run proxmark3 directly, bypassing the shell plugin. Tries `/opt/homebrew/bin/proxmark3` and `/usr/local/bin/proxmark3`.
- **Execution order**: On macOS, native spawn is tried first in `execute_pm3()`; if it succeeds, the shell plugin is never used. Requires Proxmark3 installed via Homebrew (`brew install rfidresearchgroup/proxmark3/proxmark3`).

---

## 3. Serial Port & Device Detection (Rust)

### 3.1 Port Regex — `firmware.rs` and `connection.rs`

**Before:**
```regex
^(COM[1-9]\d*|/dev/tty(ACM|USB)\d{1,2}|/dev/tty\.usbmodem\w+)$
```

**After:**
```regex
^(COM[1-9]\d*|/dev/tty(ACM|USB)\d{1,2}|/dev/(tty|cu)\.usbmodem\w+)$
```

- **Change**: Added support for `/dev/cu.usbmodem*` (macOS calling-unit devices). Some macOS setups expose serial ports as `cu.*` rather than `tty.*`; both are now accepted.

### 3.2 Platform-Specific Error Hints (`connection.rs`)

When no Proxmark3 is found, the backend now emits platform-specific troubleshooting hints:

| Platform | Hints |
|----------|-------|
| **Windows** | Check Device Manager for COM port; PM3 Easy may need CH340 driver (wch-ic.com) |
| **macOS** | Check System Information > USB; PM3 Easy may need CH340 driver (wch-ic.com/downloads); try `ls /dev/tty.usbmodem*` in Terminal |
| **Linux** | Check dmesg or lsusb; add user to `dialout` group |

### 3.3 Dynamic Port Discovery on macOS (`connection.rs`)

**Before**: Hardcoded list of `/dev/tty.usbmodem*` suffixes:
- `iceman1`, `14101`, `14201`, `14301`, `1`, `2`, `3`

**After**:
1. **Dynamic scan**: Reads `/dev/` and collects all `tty.usbmodem*` devices — works with any PM3 serial number.
2. **Fallback suffixes**: Keeps the well-known suffixes above in case the scan races with device enumeration.
3. **`cu.usbmodem*` support**: Also scans for `cu.usbmodem*` devices and adds them to the candidate list.

---

## 4. Firmware Flash (`firmware.rs`)

- **Port regex**: Added `/dev/cu.usbmodem*` support (same as connection.rs).
- **Logic**: Sidecar first, then scope fallback — unchanged from master; simplified structure.

---

## 5. Frontend — Error Step Hints (`ErrorStep.tsx`)

### 5.1 Platform-Aware Troubleshooting

**Before**: Single static list of Windows-focused hints:
- Try a different USB cable
- Check Device Manager for COM port
- PM3 Easy CH340 driver
- Antivirus blocking proxmark3.exe

**After**: `getDetectHints()` returns hints based on `navigator.platform`:

| Platform | Hints |
|----------|-------|
| **macOS** | Install Proxmark3: `brew install rfidresearchgroup/proxmark3/proxmark3`; USB cable; System Information → USB; CH340 (wch-ic.com/downloads); `ls /dev/tty.usbmodem*` in Terminal |
| **Windows** | USB cable; Device Manager; CH340 (wch-ic.com); Antivirus exceptions |
| **Other** | USB cable; dmesg/lsusb; dialout group; CH340 driver |

---

## 6. Documentation

### 6.1 README.md

- **Badges**: Added macOS 10.15+ badge.
- **Requirements**: Added macOS 10.15+ (Intel or Apple Silicon) and macOS-specific note (CH340 driver, Homebrew PM3 path).
- **Installation**: Split into Windows and macOS sections with platform-specific steps.
- **Build**: Updated to mention DMG on macOS; added macOS build note: install Proxmark3 via Homebrew (`brew install rfidresearchgroup/proxmark3/proxmark3` or `brew tap proxmark/proxmark3 && brew install proxmark3`). The bundled sidecar has Homebrew dylib dependencies not included in the DMG.
- **Wording**: "DLLs" → "libraries" for cross-platform accuracy.

### 6.2 New Files

| File       | Purpose |
|------------|---------|
| **AGENTS.md** | Repository guidelines for AI agents: structure, commands, style, testing, PRs, security. |
| **CLAUDE.md** | Claude-specific project overview, architecture, conventions, IPC, PM3 layer, database. |
| **GEMINI.md** | Gemini-specific project overview, build/run instructions, conventions, tech stack. |

### 6.3 .gitignore

- **Removed**: `CLAUDE.md` and `.claude/` — these files are now tracked in the repo.

---

## 7. File Summary

| Action | Path |
|--------|------|
| Modified | `.gitignore`, `README.md`, `package-lock.json` |
| Modified | `src-tauri/tauri.conf.json`, `src-tauri/src/commands/firmware.rs`, `src-tauri/src/pm3/connection.rs` |
| Modified | `src/components/wizard/ErrorStep.tsx` |
| Added | `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `CHANGES-macOS.md` |
| Added | `src-tauri/Entitlements.plist`, `src-tauri/binaries/proxmark3-aarch64-apple-darwin` (binary) |

---

## 8. Testing Recommendations

- **macOS DMG**: Build with `npx tauri build`, install Proxmark3 via Homebrew, run the .app from the DMG. Device detection should work via native spawn.
- **macOS dev**: Run `npx tauri dev`, connect PM3, verify device detection and cloning flow.
- **macOS (Intel)**: Same flow with `proxmark3-x86_64-apple-darwin` if building for x64.
- **Windows**: Ensure existing behavior unchanged; `bundle.targets: "all"` should still produce NSIS.
- **Error hints**: Trigger "device not found" on each platform and confirm correct hints appear.
