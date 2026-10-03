# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Phosphor is a desktop GUI for the Proxmark3 RFID/NFC tool. It wraps the Iceman fork's `proxmark3` CLI in a wizard UI for one-click LF (125 kHz) and HF (13.56 MHz) card cloning. Tauri v2 (Rust backend) + React 19 + XState v5 + TypeScript + Tailwind 4. Windows ships as an NSIS installer with a bundled client; macOS (`.app`/`.dmg`) and Linux builds use an installed client.

## Commands

| Task | Command |
| --- | --- |
| Full dev (Vite + Rust window) | `npx tauri dev` |
| Production build | `npx tauri build` (NSIS on Windows, `.app`/`.dmg` on macOS) |
| Frontend-only build | `npm run build` (= `tsc && vite build`) |
| Frontend tests (Vitest) | `npm test` |
| Rust tests | `cd src-tauri && cargo test` |
| All checks | `./scripts/check.sh` or `pwsh scripts/check.ps1` (vitest → tsc → vite build → cargo test) |
| Release artifact check | `npm run release:check` |

CI (`.github/workflows/ci.yml`) runs the same checks on Linux, Windows and macOS.

Bundle config is split per platform. `tauri.conf.json` is platform-neutral (firmware resources only). `tauri.windows.conf.json` adds the NSIS target, the `proxmark3` sidecar and the DLL resources; `tauri.macos.conf.json` sets `.app`/`.dmg`. Tauri merges these with JSON Merge Patch: objects merge key by key, so `{}` does **not** clear a key (use `null`).

## Required external binaries (gitignored)

Production builds require artifacts that are **not in the repo** (GPL upstream):

- `src-tauri/binaries/proxmark3-x86_64-pc-windows-msvc.exe` — Windows sidecar, Iceman fork release
- `src-tauri/pm3-libs/*.dll` — Qt5, ICU, MinGW runtime DLLs that the sidecar links against. **DLLs must sit next to the exe, not in a subdirectory** (the Windows loader requires this — see commit `ef1cef4`).
- `src-tauri/firmware/{rdv4,rdv4-bt,generic,generic-256}/fullimage.elf` — proxmarkbuilds.org. Optional for dev builds (the `firmware` dir is committed with a `.gitkeep`).

Only Windows builds need the sidecar and DLLs. See `docs/releasing.md`.

## Architecture: dual state machine

The wizard is implemented as **two parallel finite state machines that must stay in sync**:

1. **Rust authoritative FSM** — `src-tauri/src/state.rs::WizardMachine`. Owns the real state; rejects invalid transitions; persists device info (port/model/firmware) across `BackToScan` / `SoftReset` / `CancelHfProcess`.
2. **XState v5 mirror** — `src/machines/wizardMachine.ts`, hosted by `src/hooks/WizardProvider.tsx`. Drives the React UI.

Sync rule: every user-driven transition first calls the Rust FSM via the `wizard_action` Tauri command (or a higher-level command like `scan_card` that transitions internally), then sends the matching XState event. On a Rust failure, `WizardProvider` resets both sides — never let them diverge.

When **adding or renaming a wizard step**, you must update all of:

- `WizardState` and `WizardAction` enums in `src-tauri/src/state.rs` (and the transition match arm)
- `WizardStepName` union in `src/machines/types.ts`
- States + events + actor in `src/machines/wizardMachine.ts`
- `STATE_TO_STEP` map + (if async) `isLoading` set in `src/hooks/WizardProvider.tsx`
- Map the `UserAction` variant in `src-tauri/src/commands/wizard.rs` if the frontend needs to trigger it directly. Internal-only actions (`DeviceFound`, `CardFound`, `WriteFinished`, etc.) deliberately have no `UserAction` mapping — they're triggered from inside backend commands.

## Architecture: PM3 subprocess execution

All `proxmark3` invocations funnel through `src-tauri/src/pm3/connection.rs` (`spawn_pm3` / `spawn_binary` are the only spawn sites). **Do not spawn the client from anywhere else** — the chokepoint enforces:

- Port validation via `pm3/ports.rs::is_valid_port` (COM*, /dev/ttyACM*, /dev/ttyUSB*, /dev/rfcomm*, /dev/tty.* and /dev/cu.*) — a port is its own argv entry but must never look like a flag.
- Rejection of `;`, `\n`, `\r` in the command string — the PM3 CLI's `-c` flag treats `;` as a delimiter, so `"AA;lf t55xx wipe"` would execute two commands.
- Client lookup via `pm3/binary.rs::resolve()`, which always yields an **absolute path**: custom path from Settings → bundled sidecar next to the app exe (Tauri flattens `externalBin` there) → `PATH` → known install locations (`known_locations()`). Add new install locations there. Tauri shell-scope names are not used: capabilities only gate frontend IPC, and the webview has no shell permissions.

Execution modes:

- **`run_command()`** — short ops, 30 s timeout (`PM3_COMMAND_TIMEOUT`); the client is **killed** on timeout so it can't keep the serial port open. Not externally cancellable; fine because LF writes finish in < 2 s.
- **`run_command_streaming()`** — long ops (autopwn); parks the `CommandChild` in `HfOperationState.child` so `cancel_hf_operation` can kill it. `timeout_secs` is an inactivity limit. Each line goes to a `pm3-output` event AND the `on_line` callback.
- **`run_flash()`** — firmware flash, same streaming/cancel mechanics with `FlashState.child`.
- A streaming run whose child was taken by a cancel command returns `AppError::Cancelled`. Callers must not transition the wizard on `Cancelled` — the frontend's cancel action owns that transition (the XState `hfProcessing` state maps a `"Cancelled"` rejection to `deviceConnected`).

Port detection (`detect_device`) probes in `ports::detection_order()` order: preferred port from Settings, enumerated ports with a Proxmark3 USB ID (9AC4:4B8F, 2D2D:504D), other USB serial, Bluetooth, then legacy fixed guesses.

Tests: `connection.rs::process_tests` runs a fake `proxmark3` shell script through a mock Tauri app (Unix only) — extend it when changing process lifecycle behaviour.

PM3 commands are constructed in `src-tauri/src/pm3/command_builder.rs` (with per-card-type hex/format validation) and the noisy CLI output is parsed in `src-tauri/src/pm3/output_parser.rs` (~150 KB — heavy regex matching of PM3 stdout).

## Tauri ↔ React event channel

Long-running commands emit Tauri events; the frontend listens in `WizardProvider`:

| Event | Emitted by | Consumed for |
| --- | --- | --- |
| `pm3-output` | `connection.rs::emit_output()` | Live terminal panel (`LiveTerminal.tsx`) |
| `write-progress` | `commands/write.rs` | LF write progress bar |
| `hf-progress` | `commands/hf_clone.rs` | HF autopwn / dump progress |
| `firmware-progress` / `-complete` / `-failed` | `commands/firmware.rs` | Firmware flash UI (no completion event after a cancel) |

The typed Tauri command wrappers live in `src/lib/api.ts` — every `invoke()` call in the frontend should go through there, not be inlined.

## Persistence

SQLite via `rusqlite` (bundled) at `<app_data_dir>/phosphor.db`. Schema in `src-tauri/src/db/mod.rs`:

- `clone_log` — history of clone operations
- `saved_cards` — user-saved card dumps for re-cloning later
- `app_settings` — key/value; key `pm3` holds the PM3 runtime settings JSON (preferred port, custom client path)

The `Database` handle is managed by Tauri (`app.manage(database)`); access via `State<'_, Database>` in command handlers. PM3 settings are loaded into `settings::SettingsState` at startup so `connection.rs` can read them without the frontend; UI-only preferences (expert mode) stay in `localStorage`.

## "Capabilities mismatch" is not a failure

If `hw version` returns a capabilities-mismatch error during device detection, the device IS present — the bundled client just doesn't match the device's firmware version. `connection.rs::detect_device()` treats this as success with `firmware = "mismatched"`, and the wizard routes into the firmware-flash flow rather than reporting "no device".

## CSP

`tauri.conf.json` sets a strict CSP that allows only `'self'` for scripts. Loading remote JS will break the production build silently. Fonts from `fonts.googleapis.com` / `fonts.gstatic.com` are the only exception.
