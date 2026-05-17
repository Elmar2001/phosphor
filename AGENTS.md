# Repository Guidelines

## Project Structure & Module Organization

Phosphor is a Tauri v2 desktop app with a React/TypeScript frontend and Rust backend. Frontend code lives in `src/`: reusable UI in `src/components/`, React hooks/providers in `src/hooks/`, shared constants and API helpers in `src/lib/`, XState wizard logic in `src/machines/`, and global Tailwind/CSS variables in `src/styles/globals.css`. Static audio assets are in `src/assets/`.

Rust code lives in `src-tauri/src/`. Tauri command modules are grouped under `src-tauri/src/commands/`; Proxmark3 integration is in `src-tauri/src/pm3/`; card models, database code, app state, and errors are split into `cards/`, `db/`, `state.rs`, and `error.rs`. Tauri configuration, icons, bundled PM3 resources, and installer settings are under `src-tauri/`.

## Build, Test, and Development Commands

- `npm install`: install Node and Tauri CLI dependencies.
- `npm run dev`: start the Vite frontend on port `1420`.
- `npx tauri dev`: run the full desktop app in development.
- `npm run build`: run `tsc` and build the frontend bundle.
- `npx tauri build`: create the production Tauri/NSIS bundle.
- `pwsh ./scripts/check.ps1`: run TypeScript checks, `cargo check`, and Vite build.
- `cd src-tauri && cargo check`: validate Rust backend compilation only.

## Coding Style & Naming Conventions

TypeScript is strict (`noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`). Keep React components in PascalCase files, hooks as `useX`, and provider components as `XProvider`. Follow nearby import and quote style when editing.

Rust uses edition 2021 conventions: `snake_case` modules/functions, `PascalCase` types, and Tauri commands exposed through `commands/mod.rs` and `generate_handler!`. Run `cargo fmt` before larger Rust changes. Preserve the terminal/CRT visual language in CSS, including the global zero-radius rule.

## Testing Guidelines

There is no committed JS test runner or dedicated test suite yet. Treat `pwsh ./scripts/check.ps1` as the baseline verification before opening a PR. For backend logic, prefer focused Rust unit tests near the module being changed and run `cd src-tauri && cargo test`. For frontend state changes, verify both `npm run build` and the live app via `npx tauri dev`.

## Commit & Pull Request Guidelines

Recent commits use short, imperative summaries such as `Fix ...`, `Add ...`, `Update ...`, and versioned release commits like `Phosphor v1.1.0: ...`. Keep commits scoped to one behavior or cleanup.

Pull requests should include a concise summary, verification commands run, linked issue or audit IDs when applicable, and screenshots or recordings for UI-visible changes. Note whether Proxmark3 hardware, bundled binaries, firmware resources, or installer output were tested.
