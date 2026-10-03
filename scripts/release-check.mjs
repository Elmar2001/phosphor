#!/usr/bin/env node
// Pre-release sanity check for bundled artifacts and version numbers.
//
//   npm run release:check                      # checks for the host OS
//   npm run release:check -- --platform windows
//   npm run release:check -- --allow-missing-firmware
//
// Checks file contents, not just presence: an empty placeholder or an HTML
// error page saved as fullimage.elf would pass an existence check.

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const args = process.argv.slice(2);
const flag = (name) => args.includes(`--${name}`);
const option = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
};

const hostPlatform = { win32: 'windows', darwin: 'macos' }[process.platform] ?? 'linux';
const platform = option('platform') ?? hostPlatform;
const FIRMWARE_VARIANTS = ['rdv4', 'rdv4-bt', 'generic', 'generic-256'];

let failures = 0;
let warnings = 0;
const ok = (msg) => console.log(`[OK]   ${msg}`);
const warn = (msg) => { warnings++; console.log(`[WARN] ${msg}`); };
const fail = (msg) => { failures++; console.log(`[FAIL] ${msg}`); };

/** First bytes of a file, or null if missing/empty. */
function head(path, n = 4) {
  if (!existsSync(path) || statSync(path).size === 0) return null;
  return readFileSync(path).subarray(0, n);
}

const isElf = (bytes) => bytes && bytes[0] === 0x7f && bytes.subarray(1, 4).toString() === 'ELF';
const isPe = (bytes) => bytes && bytes.subarray(0, 2).toString() === 'MZ';

console.log(`Release check for ${platform}\n`);

// -- Versions --
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const conf = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')).version;
const cargo = readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8').match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (pkg === conf && conf === cargo) ok(`version ${pkg} in package.json, tauri.conf.json, Cargo.toml`);
else fail(`version mismatch: package.json=${pkg} tauri.conf.json=${conf} Cargo.toml=${cargo}`);

// -- PM3 client --
if (platform === 'windows') {
  const sidecar = join(root, 'src-tauri/binaries/proxmark3-x86_64-pc-windows-msvc.exe');
  if (isPe(head(sidecar))) ok('PM3 sidecar is a Windows executable');
  else fail(`missing or invalid PM3 sidecar: ${sidecar}`);

  const libs = join(root, 'src-tauri/pm3-libs');
  const dlls = existsSync(libs)
    ? readdirSync(libs).filter((f) => f.toLowerCase().endsWith('.dll') && isPe(head(join(libs, f))))
    : [];
  if (dlls.length > 0) ok(`${dlls.length} runtime DLLs in src-tauri/pm3-libs`);
  else fail('no valid DLLs in src-tauri/pm3-libs (the sidecar will not start)');
} else {
  ok(`${platform} builds use an installed proxmark3 client (no sidecar bundled)`);
}

// -- Firmware --
const missing = [];
for (const variant of FIRMWARE_VARIANTS) {
  const path = join(root, 'src-tauri/firmware', variant, 'fullimage.elf');
  const bytes = head(path);
  if (isElf(bytes)) ok(`firmware ${variant}/fullimage.elf`);
  else if (bytes) fail(`firmware ${variant}/fullimage.elf is not an ELF file`);
  else missing.push(variant);
}
if (missing.length > 0) {
  const msg = `no firmware for ${missing.join(', ')}: in-app flashing is unavailable for these boards`;
  if (flag('allow-missing-firmware')) warn(msg);
  else fail(`${msg} (pass --allow-missing-firmware to release anyway)`);
}

console.log(`\n${failures} failed, ${warnings} warnings`);
process.exit(failures > 0 ? 1 : 0);
