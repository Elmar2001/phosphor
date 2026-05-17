# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Phosphor is a Windows desktop GUI for the Proxmark3 RFID/NFC tool. It wraps the Iceman fork's `proxmark3` CLI in a wizard UI for one-click LF (125 kHz) and HF (13.56 MHz) card cloning. Tauri v2 (Rust backend) + React 19 + XState v5 + TypeScript + Tailwind 4, bundled as an NSIS installer.

## Commands

| Task | Command |
| --- | --- |
| Full dev (Vite + Rust window) | `npx tauri dev` |
| Production NSIS installer | `npx tauri build` |
| Frontend-only build | `npm run build` (= `tsc && vite build`) |
| TS type-check only | `npx tsc --noEmit` |
| Rust check only | `cd src-tauri && cargo check` |
| All pre-flight checks (Windows) | `pwsh scripts/check.ps1` (tsc → cargo check → vite build) |

No test framework is wired up. Validation = the three checks above.

`bundle.targets` in `src-tauri/tauri.conf.json` is `["nsis"]` only — `tauri build` on macOS/Linux is unsupported for distribution even though `pm3` scope names exist for those platforms (dev mode works).

## Required external binaries (gitignored)

Production builds require artifacts that are **not in the repo** (GPL upstream):

- `src-tauri/binaries/proxmark3.exe` — sidecar, Iceman fork release
- `src-tauri/pm3-libs/*.dll` — Qt5, ICU, MinGW runtime DLLs that the sidecar links against. **DLLs must sit next to the exe, not in a subdirectory** (the Windows loader requires this — see commit `ef1cef4`).
- `src-tauri/firmware/{rdv4,rdv4-bt,generic}/*.elf` — proxmarkbuilds.org

`.gitignore` also excludes `CLAUDE.md` and `.claude/` — local-only.

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

All `proxmark3` invocations funnel through `src-tauri/src/pm3/connection.rs`. **Do not call `Command::new("proxmark3")` from anywhere else** — the chokepoint enforces:

- Port format validation via `PORT_RE` regex (COM*, /dev/ttyACM*, /dev/tty.usbmodem*) — blocks command injection through subprocess args.
- Rejection of `;`, `\n`, `\r` in the command string — the PM3 CLI's `-c` flag treats `;` as a delimiter, so `"AA;lf t55xx wipe"` would execute two commands.
- Sidecar-first lookup (`binaries/proxmark3`), falling back to platform-specific absolute paths registered as named scopes in `src-tauri/capabilities/default.json` (`proxmark3-win-c`, `proxmark3-mac-brew`, etc.). If you add a new install location, register it in both that file and `pm3_scope_names()`.
- 30-second timeout (`PM3_COMMAND_TIMEOUT`) for one-shot `run_command()`.

Two execution modes:

- **`run_command()`** — short ops; uses `.output()`, returns full stdout. Cannot be cancelled externally (the shell plugin owns the child); fine because LF writes finish in < 2 s.
- **`run_command_streaming()`** — long ops (autopwn, dump, firmware flash); uses `.spawn()`, stores the `CommandChild` in shared state (`HfOperationState.child` / `FlashState`) so `cancel_hf_operation` / `cancel_flash` can `kill()` it mid-run. Each output line is emitted as a `pm3-output` event AND passed to an `on_line` callback for real-time progress parsing.

PM3 commands are constructed in `src-tauri/src/pm3/command_builder.rs` (with per-card-type hex/format validation) and the noisy CLI output is parsed in `src-tauri/src/pm3/output_parser.rs` (~150 KB — heavy regex matching of PM3 stdout).

## Tauri ↔ React event channel

Long-running commands emit Tauri events; the frontend listens in `WizardProvider`:

| Event | Emitted by | Consumed for |
| --- | --- | --- |
| `pm3-output` | `connection.rs::emit_output()` | Live terminal panel (`LiveTerminal.tsx`) |
| `write-progress` | `commands/write.rs` | LF write progress bar |
| `hf-progress` | `commands/hf_clone.rs` | HF autopwn / dump progress |
| `firmware-progress` / `-complete` / `-failed` | `commands/firmware.rs` | Firmware flash UI |

The typed Tauri command wrappers live in `src/lib/api.ts` — every `invoke()` call in the frontend should go through there, not be inlined.

## Persistence

SQLite via `rusqlite` (bundled) at `<app_data_dir>/phosphor.db`. Schema in `src-tauri/src/db/mod.rs`:

- `clone_log` — history of clone operations
- `saved_cards` — user-saved card dumps for re-cloning later

The `Database` handle is managed by Tauri (`app.manage(database)`); access via `State<'_, Database>` in command handlers.

## "Capabilities mismatch" is not a failure

If `hw version` returns a capabilities-mismatch error during device detection, the device IS present — the bundled client just doesn't match the device's firmware version. `connection.rs::detect_device()` treats this as success with `firmware = "mismatched"`, and the wizard routes into the firmware-flash flow rather than reporting "no device".

## CSP

`tauri.conf.json` sets a strict CSP that allows only `'self'` for scripts. Loading remote JS will break the production build silently. Fonts from `fonts.googleapis.com` / `fonts.gstatic.com` are the only exception.
