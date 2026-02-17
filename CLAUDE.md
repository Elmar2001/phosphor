# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Phosphor is a cross-platform desktop GUI for Proxmark3 (RFID/NFC research tool). It wraps the PM3 CLI client in a point-and-click interface for cloning RFID/NFC cards. Built with Tauri v2 (Rust backend + React/TypeScript frontend).

## Build & Development Commands

```bash
npm install              # Install JS dependencies
npx tauri dev            # Full dev mode (Vite + Rust hot-reload)
npx tauri build          # Production build (.exe/.dmg)
npm run build            # Frontend only (tsc + vite build)
npx tsc --noEmit         # TypeScript type check
cd src-tauri && cargo check   # Rust type check
```

There are no automated tests in this codebase. The closest CI check is `scripts/check.ps1` (Windows only): runs tsc, cargo check, and vite build.

## Architecture

### Dual State Machine

The core pattern is a **mirrored FSM** on both sides of the IPC boundary:

- **Rust FSM** (`src-tauri/src/state.rs`): Hand-written enum-based state machine (`WizardState`, `WizardAction`) wrapped in `Mutex<WizardMachine>`. All transitions are explicit match arms; invalid ones return `AppError::InvalidTransition`.
- **Frontend FSM** (`src/machines/wizardMachine.ts`): XState v5 machine mirroring the Rust states. Each async step invokes a Tauri backend command via typed wrappers.

State flow: `Idle` → `DetectingDevice` → `DeviceConnected` → `ScanningCard` → `CardIdentified` → (LF: `WaitingForBlank` / HF: `HfProcessing` → `HfDumpReady`) → `BlankDetected` → `Writing` → `Verifying` → `VerificationComplete` → `Complete`

### IPC Pattern

- Frontend calls `invoke<T>()` via typed wrappers in `src/lib/api.ts` — never call `invoke()` directly from components.
- Backend commands return `Result<WizardState, AppError>`.
- Long-running operations (HF autopwn, firmware flash) stream progress via Tauri events (`pm3-output`, `hf-progress`, `firmware-progress`) listened to in `WizardProvider`.
- `UserAction` enum in `wizard.rs` is a restricted subset of `WizardAction` — prevents frontend from triggering backend-only transitions.

### PM3 Subprocess Layer (`src-tauri/src/pm3/`)

- `connection.rs`: Spawns PM3 as a Tauri sidecar subprocess with stdout/stderr streaming and 30s timeout.
- `command_builder.rs`: Constructs PM3 CLI commands with regex-validated inputs (injection-safe).
- `output_parser.rs`: Strips ANSI codes and parses PM3 output with regex. Large file (~150KB).
- `version.rs`: Parses `hw version` output, detects hardware variant.
- Platform-aware binary resolution: tries PATH first, then platform-specific absolute paths.

### Database

SQLite via rusqlite (bundled) at `{app_data_dir}/phosphor.db`. Two tables: `clone_log` (history), `saved_cards` (card library). Protected by `Mutex<Connection>`.

### Frontend Architecture

- React 19 + Tailwind CSS v4 + Vite v7 (port 1420)
- Single `WizardProvider` at app root provides XState machine via `useWizard()` hook
- Settings via `useSettings` context (localStorage), terminal output via `useTerminalLog` context
- Matrix/CRT aesthetic: IBM Plex Mono font, green palette (`--green-bright: #00FF41`), no border-radius, ASCII status symbols

## Key Conventions

### TypeScript
- Strict mode with `noUnusedLocals` and `noUnusedParameters`
- Types in `src/machines/types.ts` mirror Rust enums exactly — keep them in sync
- Tagged union for `WizardState`: `{ step: 'Idle' } | { step: 'DeviceConnected'; data: DeviceInfo } | ...`
- `StepName` uses PascalCase to match Rust enum variant names

### Rust
- `#[serde(tag = "action", content = "payload")]` on action enums (adjacently tagged for JS)
- `serde(rename_all = "camelCase")` on structs crossing the IPC boundary
- `LazyLock<Regex>` for all compiled regexes
- `thiserror` for error types; `AppError` implements `Serialize` for Tauri IPC
- Commands lock the `WizardMachine` mutex, do work, then return the current state

### Sidecar Binaries
PM3 binaries (`src-tauri/binaries/`), DLLs (`src-tauri/pm3-libs/`), and firmware files (`src-tauri/firmware/`) are gitignored and must be placed manually.
