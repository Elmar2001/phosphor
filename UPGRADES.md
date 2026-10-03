# Upgrades

What changed on top of upstream `nikitaart2000/phosphor` v1.1.0
(`440ecfc`): working PM3 client lookup on every OS, a reliable process
layer, faster device detection, real diagnostics, persisted runtime
settings, tests and CI on Linux, Windows and macOS.

This replaces an earlier upgrade attempt (commits `6e6393a` through
`ec21323`), which was removed from `master`. See
[The earlier attempt](#the-earlier-attempt) for why.

## At a glance

| Area | Before | Now |
| --- | --- | --- |
| Finding the PM3 client | Only `proxmark3` on `PATH` worked; install-location fallbacks were broken | Custom path → bundled client → `PATH` → known locations, always as an absolute path |
| macOS | Homebrew client not found when launched from Finder | Found in `/opt/homebrew`, `/usr/local`, `/opt/local`; `.app`/`.dmg` build config |
| Stuck PM3 commands | Left running after the 30 s timeout, holding the serial port | Killed on timeout |
| Device detection | Spawned the client for COM1..COM40 in order | Enumerates ports, probes the Proxmark3 (by USB ID) first |
| Firmware flash | Cancel did nothing; `generic-256` firmware never bundled | Cancellable, one flash at a time, live output and progress, all variants bundled |
| Cancelling MIFARE key recovery | Could desync the Rust and XState wizards | Clean return to the device screen |
| Setup problems | "Device not found" for everything | DIAG tab: client launch check, real port list, firmware, overrides, copyable report |
| Overrides | None | Preferred port and custom client path in Settings, stored in the database |
| Webview permissions | Could spawn `proxmark3` with any arguments | No shell permissions |
| Tests | 225 Rust parser/builder tests | 272 Rust tests (incl. fake-client process tests) + 24 frontend tests |
| CI | None | Linux, Windows, macOS on every push to `master` and every PR |

## Changes in detail

### PM3 client lookup — `dee81d9`

Upstream passed Tauri shell-scope *names* (`proxmark3-mac-brew`,
`proxmark3-win-c`, ...) to `app.shell().command()`. Those names are only
resolved for frontend IPC; from Rust they were run as literal program
names, so every fallback failed and only `proxmark3` on `PATH` worked.
Apps opened from Finder don't have Homebrew on `PATH`, so macOS never found
the client. The bundled sidecar was also looked up in `<exe dir>/binaries/`,
but Tauri installs it directly next to the executable; Windows only worked
because Rust's `Command` happens to search the app directory for bare names.

New `src-tauri/src/pm3/binary.rs` resolves the client to an absolute path,
first match wins:

1. Custom path from Settings. It must be absolute, exist, be executable,
   and be named `proxmark3`/`proxmark3.exe`, so the setting can't launch
   arbitrary programs.
2. Bundled client next to the app executable.
3. `proxmark3` on `PATH`.
4. Known install locations: `C:\proxmark3`, `C:\Program Files\proxmark3`;
   `/opt/homebrew/bin`, `/usr/local/bin`, `/opt/local/bin` (MacPorts, new);
   `/usr/local/bin`, `/usr/bin`.

### Process lifecycle — `dee81d9`, `d0cc051`, `2f10938`

All launches go through one spawn function in `pm3/connection.rs`.

- **One-shot commands** (`run_command`) kill the client when the 30 s
  timeout fires. Before, the client kept running and held the serial port,
  so the next commands failed too.
- **Streaming commands** (autopwn, firmware flash) park the child process
  where the cancel commands can reach it. The client is killed on
  inactivity timeout or pipe error. A run whose child was killed by a cancel
  returns the new `AppError::Cancelled`.
- A spawned streaming client is always parked before anything can return
  early, so no process is left running that nothing can cancel.
- A streaming client killed by a signal now counts as a failure; it used to
  be reported as success.
- **No lost trailing output.** The shell plugin can report a fast-exiting
  process as terminated before its last lines arrive. The streaming reader
  used to stop at that point and drop them, so a caller could miss lines it
  parses, such as flash "All done" or autopwn's dump file. It now keeps
  reading until the pipes close, for at most 2 s.

### Device detection — `dee81d9`

New `pm3/ports.rs` enumerates serial ports with the `serialport` crate
(no libudev, so no extra Linux system packages) and probes them in this
order:

1. Preferred port from Settings
2. Ports whose USB ID is a Proxmark3 (`9AC4:4B8F`, `2D2D:504D`), or whose
   name or product mentions proxmark/iceman
3. Other USB serial ports
4. Bluetooth and unidentified ports (on-board PCI UARTs are skipped)
5. The old fixed list (COM1..COM40, ttyACM0..5, ...) as a fallback tail,
   so nothing that was found before is missed

macOS lists every device twice (`/dev/tty.X` and `/dev/cu.X`); only the
`tty.` twin is probed. Valid port names now also include `/dev/cu.*`,
`usbserial`/`wchusbserial`/`SLAB_USBtoUART` macOS names, `/dev/rfcomm*`
(RDV4 Bluetooth add-on) and COM ports up to 999. A port still can't start
with `-` or contain separators. Detection prints which client it is using
and flags Proxmark3 USB devices in the terminal.

### Firmware flash — `dee81d9`, `ae4dc47`, `d0cc051`

- **Cancel works.** Upstream used `.output()`, so the process handle was
  never stored: CANCEL moved the UI on while the flash kept running. The
  flash now streams through `connection::run_flash` and can be killed. A
  note explains that a PM3 left in bootloader mode recovers by flashing
  again.
- **One flash at a time**, enforced for the whole command, including before
  the process starts.
- **Live output** in the terminal, and progress milestones parsed from
  client output (bootloader → writing → done) that only move the bar
  forward.
- **`generic-256` firmware is bundled.** The variant was detected and
  accepted for flashing but never packaged, so its flash always failed with
  "file not found". Firmware is now bundled as one `firmware` directory.

### Wizard state sync on cancel — `dee81d9`, `af76184`

Cancelling autopwn killed the client, `hf_autopwn` then moved the Rust
wizard to Error (valid from any state), racing the frontend's
`CancelHfProcess`. The two state machines could end up in different states.
Now `hf_autopwn` leaves the state alone on `Cancelled`, and the XState
`hfProcessing` state treats a `"Cancelled"` rejection like `CANCEL_HF`.

### Diagnostics tab — `64d33a9`

New **DIAG** tab (`get_pm3_diagnostics`). It never touches the device:

- **PM3 client:** resolves it like real commands do and launches it with
  `--version`. Reports the client version, or why it can't start, with
  specific hints for a missing DLL (`0xC0000135`) or wrong CPU architecture
  (`0xC000007B`). Lists every lookup candidate and which one is in use.
- **Serial ports:** the real list with USB IDs, Proxmark3 devices flagged,
  and a warning if the preferred port isn't plugged in.
- **Custom client path** still valid, and **bundled firmware** variants.
- An overall OK/WARNING/ERROR status computed in the backend, and
  **COPY REPORT** for bug reports.

### PM3 runtime settings — `dee81d9`, `39b826c`

Settings > **PM3 RUNTIME**:

- **Preferred port:** picker filled from the enumerated ports, with
  Proxmark3 devices labelled, a RESCAN button, and a saved-but-unplugged
  port kept selectable.
- **PM3 client path:** for self-built or unusual installs.

Saving is explicit. The backend validates both values and the error is
shown. Settings are stored in a new `app_settings` table in `phosphor.db`
and loaded at startup, so detection uses them before the UI has loaded.
Existing databases get the table automatically. Expert mode stays in
`localStorage`.

### Troubleshooting hints — `9f500a7`

The detection error screen gave Windows advice on every OS. macOS and
Linux now get their own steps: the Homebrew install command, System
Information, the `dialout` group and ModemManager.

### Security — `6e4abb5`

The webview had `shell:allow-execute`, `shell:allow-spawn` (with
`args: true`) and `shell:allow-kill`, letting page JavaScript run
`proxmark3` with any arguments, including `--flash`, and bypass the
backend's port and command validation. The frontend never used them.
They're removed along with the unused `@tauri-apps/plugin-shell` npm
package. The Rust plugin is unaffected; capabilities only gate frontend
calls.

### Build configuration — `ae4dc47`

- `tauri.conf.json` is platform-neutral (firmware only, all bundle
  targets).
- `tauri.windows.conf.json` adds NSIS, the `proxmark3` sidecar and the
  DLLs. The effective Windows config is unchanged.
- `tauri.macos.conf.json` builds `.app`/`.dmg` with macOS 10.15 as the
  minimum (Tauri v2's supported floor).
- macOS and Linux build with no GPL artifacts present; only Windows needs
  the sidecar and DLLs.

### Tests — `dee81d9`, `af76184`, `64d33a9`, `9f500a7`

- **Rust: 272** (225 upstream + 47 new), covering client lookup and path
  validation, port validation and probe order, settings, diagnostics checks,
  flash milestones and stream event ordering.
- **Fake-client process tests** (`connection.rs::process_tests`, Unix):
  run a fake `proxmark3` shell script through a mock Tauri app to check
  timeout kills, cancel, inactivity timeout, detection (including
  capabilities mismatch), the flash argument order and the `--version`
  probe.
- **Frontend: 24** (Vitest). `wizardMachine.test.ts` drives the XState
  wizard with stubbed backend calls: detection, firmware mismatch/skip,
  error paths, the full LF scan → blank → write → verify → complete flow and
  its guards, and HF key recovery including cancel. Plus report formatting,
  error unwrapping and platform hints.

### CI and scripts — `f87e5a6`

- **CI** (`.github/workflows/ci.yml`) runs frontend tests, type check and
  build, and `cargo test --locked` on Linux, Windows and macOS for every
  push to `master` and every PR. Windows gets empty stand-ins for the GPL
  sidecar and DLLs.
- **`scripts/check.sh`** (new) and **`scripts/check.ps1`** run the same
  checks locally.
- **`npm run release:check`** checks that versions match across
  `package.json`, `tauri.conf.json` and `Cargo.toml`. It also checks that
  the Windows sidecar and DLLs are real executables and each firmware image
  is a real ELF file, which catches empty placeholders and saved error
  pages.

### Documentation — `85e0a59`

- `docs/platform-setup.md`: client lookup order; Windows, macOS and Linux
  setup (Homebrew, `dialout`, ModemManager udev rule); detection order and
  overrides.
- `docs/releasing.md`: artifacts that aren't in the repo, the release
  checklist and the hardware smoke test.
- README, CLAUDE.md and AGENTS.md updated to match.

## Dependencies

- Added `serialport` 4.10 (Rust, default features off)
- Added the Tauri `test` feature (Rust, dev only)
- Added `vitest` 5 (npm, dev)
- Removed `@tauri-apps/plugin-shell` (npm)

## Verification

- **CI** on `master` at `d0cc051`
  ([run #2](https://github.com/Elmar2001/phosphor/actions/runs/37129858762)):
  passed on Linux (3.0 min), macOS (3.4 min) and Windows (4.5 min).
- **Flaky test found and fixed:** the next run
  ([run #3](https://github.com/Elmar2001/phosphor/actions/runs/37130517992))
  failed on Linux in `flash_passes_port_positionally`. That exposed the
  trailing-output race above, fixed in `2f10938`. Before the fix it failed
  about 1 run in 20 under parallel load; after it, 300/300 runs passed.
- **Locally (Linux):** `./scripts/check.sh` passes, and `cargo check`
  passes with no sidecar, DLL or firmware files present.
- **Fixes proven by their tests:** the timeout-kill, cancelled-autopwn and
  trailing-output tests were checked to fail with their fix reverted.
- **Not yet tested:** real Proxmark3 hardware on any OS, and built
  Windows/macOS installers. `docs/releasing.md` has the hardware checklist.

## The earlier attempt

The first upgrade pass (`6e6393a`..`ec21323`) was removed from `master`.
Its CI ran once and failed on Windows. Several of its features reported
false results because of the upstream lookup bugs above.

| Item | Verdict | Replaced by |
| --- | --- | --- |
| macOS support | Right goal, broken | Client lookup fix and platform config split. Its macOS config used `"resources": {}`, which Tauri's merge ignores, so `.gitkeep` placeholders were needed. It also claimed macOS 10.13. |
| Fake PM3 harness | Not useful | Fake-client process tests. The original was static strings tested only against themselves. |
| Vitest | Kept | Tests that cover real logic |
| Diagnostics | Reported false results | Launch check and real port list. "Bundled sidecar OK" could never fail, and the port list was a synthetic COM1..COM40. |
| Runtime preferences | Buggy | Persisted, validated settings. It synced on every keystroke with errors hidden, and its binary dropdown used the broken scope names. |
| CI | Failed on Windows | Three-OS CI with Windows stubs |
| Release preflight | Low value | `release:check`. The original was bash-only and printed warnings only. |
| Firmware packaging | Partly right | Kept the `generic-256` fix as a directory mapping |
| Platform helpers | Minor | Detection hints only |
| Diagnostics report | Fine | Kept, with more detail |

## Next steps

- **Release workflow (CD):** none exists on `master`. A tag-triggered
  draft release could build the macOS DMG now. Windows needs the Iceman
  client, DLLs and firmware fetched from a pinned release.
- **Dev-mode fake client** with recorded outputs for `lf search`,
  `hf search`, autopwn and T5577 write/verify, to use the whole UI without
  hardware.
- **Test that the Rust and XState wizard transition tables agree.**
