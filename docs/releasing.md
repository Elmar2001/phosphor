# Releasing

## Artifacts that aren't in the repo

The PM3 client and firmware are GPL build artifacts from the Iceman
project and are not committed.

| Path | Needed for | Source |
| --- | --- | --- |
| `src-tauri/binaries/proxmark3-x86_64-pc-windows-msvc.exe` | Windows | Iceman release / ProxSpace build |
| `src-tauri/pm3-libs/*.dll`, `src-tauri/pm3-libs/platforms/*.dll` | Windows | Qt5, ICU, MinGW runtime the client links against |
| `src-tauri/firmware/{rdv4,rdv4-bt,generic,generic-256}/fullimage.elf` | In-app firmware flash | proxmarkbuilds.org, matching the bundled client version |

DLLs must sit next to the exe after install (the `pm3-libs/*` → `./`
mapping in `tauri.windows.conf.json`); the Windows loader won't find them
in a subdirectory. Firmware must match the bundled client version, or the
app will flag every device as mismatched right after flashing it.

macOS and Linux builds don't bundle a client. Bundling one on macOS would
also mean shipping and relinking its Homebrew dylibs.

## Checklist

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json`.
2. `npm run release:check` (add `--platform windows` when preparing Windows
   artifacts from another OS). It checks the versions agree and that the
   sidecar and DLLs are real PE files and each firmware image is an ELF.
   Use `--allow-missing-firmware` to ship without some variants.
3. `./scripts/check.sh` or `pwsh scripts/check.ps1`.
4. `npx tauri build`.
5. On real hardware, with the built installer/app:
   - Install, launch, and check DIAG: client starts, version shown, PM3 port
     flagged.
   - Detect device, clone an LF card (T5577), clone a MIFARE Classic
     (autopwn → magic card) and cancel one autopwn mid-run.
   - Flash firmware on a mismatched device.
   - Uninstall.
6. Sign the installer (Windows) / sign and notarize the app (macOS). Keep
   signing credentials in the release environment, not the repo.
7. Release notes: bundled client version, firmware version and variants,
   and which platforms and hardware were smoke-tested.
