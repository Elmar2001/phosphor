# Upgrades

This branch redoes the "cross-platform, diagnosable, testable" upgrade pass
on top of upstream `nikitaart2000/phosphor` at `440ecfc`. The first attempt
(commits `6e6393a` through `ec21323`, since removed from
Elmar2001/phosphor `master`) is reviewed item by item
below, followed by what was done instead.

## Problems the first pass missed

These existed upstream. The first pass built on them without noticing, and
some of its features reported false results because of them.

1. **PM3 client fallback never worked.** `connection.rs` passed Tauri shell
   scope *names* (`proxmark3-mac-brew`, `proxmark3-win-c`, ...) to
   `app.shell().command()`. Scope names are only resolved for frontend IPC;
   from Rust they're exec'd as literal program names. Only `proxmark3` on
   `PATH` ever worked. macOS apps launched from Finder don't have Homebrew
   on `PATH`, so the Homebrew client was never found. The first pass added
   MacPorts to the same broken list and a diagnostics check that reported
   these paths "OK".
2. **Bundled sidecar path was wrong.** `sidecar("binaries/proxmark3")`
   looks in `<exe dir>/binaries/`, but Tauri puts sidecars directly next to
   the exe. Windows only worked because Rust's `Command` also searches the
   application directory for a bare `proxmark3`. A bundled macOS sidecar
   would never have been found.
3. **Timed-out PM3 processes kept running.** A command that hit the 30 s
   timeout dropped its future but never killed the child. The orphaned
   client held the serial port, so later commands failed too.
4. **Firmware flash cancel was a no-op.** `flash_firmware` used
   `.output()`, so `FlashState.child` was never set. CANCEL moved the UI on
   while the flash kept running, and the "already in progress" guard never
   fired.
5. **Cancelling autopwn could desync the two state machines.** The killed
   process made `hf_autopwn` report an error (`ReportError` is valid from
   any state), racing the frontend's `CancelHfProcess`.
6. **The webview had shell spawn rights it never used.** Capabilities
   granted `shell:allow-execute`/`allow-spawn` with `args: true`, letting
   page JavaScript run `proxmark3` with any arguments and bypass the
   backend's validation. The frontend never imports the shell plugin.

## Item-by-item

| # | First pass | Verdict | This branch |
| --- | --- | --- | --- |
| 1 | macOS support | Worth doing; broken | Real client lookup (problems 1–2). `tauri.macos.conf.json` used `"resources": {}`, which Tauri's JSON Merge Patch ignores, so macOS still needed Windows resource globs. Hence the `.gitkeep` placeholders. Config is now split per platform (`tauri.windows.conf.json` / `tauri.macos.conf.json`). macOS minimum is 10.15, Tauri v2's supported floor, not 10.13. |
| 2 | Fake PM3 harness (`fakePm3.ts`) | Not useful | Static strings tested only against themselves and used by nothing; PM3 parsing lives in Rust. Replaced by `connection.rs::process_tests`, which runs a fake `proxmark3` script through a mock Tauri app and checks timeout kills, cancel, detection and flash. |
| 3 | Vitest infrastructure | Worth doing | Kept Vitest. Its tests now cover real logic: the XState wizard flows (`wizardMachine.test.ts`, stubbed actors), error-text unwrapping, report formatting, platform hints. The "225 Rust tests" it reported already existed upstream. |
| 4 | Diagnostics backend | Concept good; results false | `sidecar(...).is_ok()` only builds a path, so "bundled sidecar OK" was always true, and so was the binary rollup. Serial "candidates" were the synthetic COM1..COM40 list, so always OK on Windows. Now: launches the resolved client with `--version` (catches missing DLLs, wrong architecture, quarantine), lists *real* ports with USB IDs, and checks firmware and overrides. Status is a Rust enum; the duplicate TS summarizer is gone. |
| 5 | DIAG screen | Worth doing | Kept, showing the new data. |
| 6 | Runtime preferences | Worth doing; buggy | Pushed to an in-memory backend on every keystroke with errors swallowed, so typing an invalid port left a different stale port active. The "binary source" dropdown listed all platforms' scope names (the broken mechanism). Now: settings persist in SQLite and load at startup. Port is a picker filled from enumerated ports. Client path is a validated absolute path to a `proxmark3` binary, which covers self-built clients. Save is explicit and errors are shown. |
| 7 | CI matrix | Worth doing; failed | Its only run failed on Windows at `cargo check`: `externalBin` requires the uncommitted sidecar binary, and nothing stubbed it. No Linux job. Now: Linux/Windows/macOS, stub Windows artifacts, rust-cache, `cargo test` instead of a 4-minute `tauri build --no-bundle`. |
| 8 | Release hardening (bash preflight + doc) | Low value | Bash-only (Windows is the release platform), and it only printed warnings. Replaced by `scripts/release-check.mjs` (`npm run release:check`). It checks versions match across the three manifests, the sidecar/DLLs are PE files, and the firmware images are ELF files. `docs/releasing.md` has a short checklist. |
| 9 | Firmware packaging | Partly right | Bundling `generic-256` was a real upstream fix. It's now done with one `firmware` directory mapping (no per-variant globs or `.gitkeep`s). The check script was folded into release-check. |
| 10 | `platformSupport.ts` | Minor | Used deprecated `navigator.platform` and its lookup order already disagreed with the backend. Now only detection hints remain (user-agent based); client lookup info comes from the backend. |
| 11 | Copy diagnostics report | Fine | Kept. Adds copy feedback, client version, the candidate in use, and USB IDs. |
| – | Docs | Inaccurate in places | `docs/platform-setup.md` covers all three OSes (Homebrew, dialout, ModemManager udev rule) and `docs/releasing.md` covers releases. CLAUDE.md/AGENTS.md describe the new mechanics; the old CLAUDE.md said it was gitignored and that no tests existed. |

## Also in this branch

- **Smarter detection.** Ports are enumerated (`serialport` crate, no
  libudev) and a Proxmark3 by USB ID is probed first, instead of spawning
  the client for COM1..COM40 in order. The old list stays as a fallback
  tail, so nothing found before is missed.
- **Wider valid port names**: `/dev/cu.*`, `usbserial`/`wchusbserial`
  macOS names, `/dev/rfcomm*` (RDV4 Bluetooth), COM numbers up to 999.
- **Flash progress milestones** parsed from client output, streamed live to
  the terminal, with a concurrency guard and a recovery note on cancel.
- **Platform-specific troubleshooting** on the detection error screen.

## Verification

- `cargo test`: 270 passed (225 upstream + 45 new, 10 of them end-to-end
  process tests with a fake client). Two tests were checked to fail when
  their fix is reverted (timeout kill, cancelled-autopwn handling).
- `npm test`: 24 passed. `npm run build`: OK.
- `cargo check` on Linux with no sidecar, DLL or firmware files present: OK.
- Not tested: real Proxmark3 hardware on any OS, Windows/macOS bundles.
  The release checklist in `docs/releasing.md` covers the hardware pass.

## Follow-ups worth considering

- A dev-mode fake client with recorded outputs for `lf search`,
  `hf search`, autopwn, T5577 write/verify, so the whole UI can be
  exercised without hardware.
- A tag-triggered release workflow (tauri-action) that fetches the pinned
  Iceman client and firmware and runs `release:check`.
- Cross-checking the Rust and XState wizard transition tables in a test.
