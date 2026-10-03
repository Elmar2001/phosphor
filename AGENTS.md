# Repository Guidelines

## Project Structure & Module Organization

Phosphor is a Tauri v2 desktop app with a React/TypeScript frontend and Rust backend. Frontend code lives in `src/`: reusable UI in `src/components/`, React hooks/providers in `src/hooks/`, shared constants and API helpers in `src/lib/`, XState wizard logic in `src/machines/`, and global Tailwind/CSS variables in `src/styles/globals.css`. Static audio assets are in `src/assets/`.

Rust code lives in `src-tauri/src/`. Tauri command modules are grouped under `src-tauri/src/commands/`; Proxmark3 integration is in `src-tauri/src/pm3/`; card models, database code, app state, and errors are split into `cards/`, `db/`, `state.rs`, and `error.rs`. Tauri configuration (with `tauri.windows.conf.json` / `tauri.macos.conf.json` platform overrides), icons, bundled PM3 resources, and installer settings are under `src-tauri/`.

## Build, Test, and Development Commands

- `npm install`: install Node and Tauri CLI dependencies.
- `npm run dev`: start the Vite frontend on port `1420`.
- `npx tauri dev`: run the full desktop app in development.
- `npm run build`: run `tsc` and build the frontend bundle.
- `npx tauri build`: create the production bundle for the current platform (NSIS on Windows, `.app`/`.dmg` on macOS).
- `npm test`: run the Vitest suite.
- `pwsh ./scripts/check.ps1` / `./scripts/check.sh`: run all checks (Vitest, `tsc`, Vite build, `cargo test`).
- `cd src-tauri && cargo check`: validate Rust backend compilation only.

## Coding Style & Naming Conventions

TypeScript is strict (`noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`). Keep React components in PascalCase files, hooks as `useX`, and provider components as `XProvider`. Follow nearby import and quote style when editing.

Rust uses edition 2021 conventions: `snake_case` modules/functions, `PascalCase` types, and Tauri commands exposed through `commands/mod.rs` and `generate_handler!`. Run `cargo fmt` before larger Rust changes. Preserve the terminal/CRT visual language in CSS, including the global zero-radius rule.

## Testing Guidelines

Run `./scripts/check.sh` (macOS/Linux) or `pwsh ./scripts/check.ps1` (Windows) before opening a PR: Vitest, `tsc`, Vite build, and `cargo test`. CI runs the same on all three platforms.

Rust unit tests live next to the code (`#[cfg(test)] mod tests`). Process lifecycle changes in `pm3/connection.rs` belong in its `process_tests` module, which drives a fake `proxmark3` script through a mock Tauri app. Frontend tests are `*.test.ts` files next to the module; wizard flows are tested in `src/machines/wizardMachine.test.ts` by stubbing the promise actors with `wizardMachine.provide()`. Real-device behaviour still needs a manual pass with `npx tauri dev`.

## Commit & Pull Request Guidelines

Recent commits use short, imperative summaries such as `Fix ...`, `Add ...`, `Update ...`, and versioned release commits like `Phosphor v1.1.0: ...`. Keep commits scoped to one behavior or cleanup.

Pull requests should include a concise summary, verification commands run, linked issue or audit IDs when applicable, and screenshots or recordings for UI-visible changes. Note whether Proxmark3 hardware, bundled binaries, firmware resources, or installer/app bundles were tested, and on which OS.
