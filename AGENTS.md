# Repository Guidelines

## Project Structure & Module Organization
- `src/`: React + TypeScript frontend (Vite).
- `src/components/`: UI split by feature (`wizard/`, `erase/`, `history/`, `settings/`, `saved/`, `layout/`, `shared/`, `matrix/`).
- `src/hooks/`: app/state hooks and providers (`useSettings`, `WizardProvider`, etc.).
- `src/lib/` and `src/machines/`: shared utilities/constants and XState machine logic.
- `src-tauri/`: Rust backend (Tauri v2). Key areas: `src/commands/`, `src/pm3/`, `src/db/`, `src/cards/`.
- `src-tauri/binaries/`, `src-tauri/pm3-libs/`, `src-tauri/firmware/`: sidecar binary and runtime resources used for packaging.

## Build, Test, and Development Commands
- `npm install`: install JS dependencies.
- `npm run dev`: run frontend only on Vite dev server.
- `npx tauri dev`: run full desktop app (frontend + Rust backend).
- `npm run build`: TypeScript compile check + production frontend bundle.
- `npx tauri build`: build installable desktop artifacts.
- `cd src-tauri && cargo check`: fast Rust validation.
- `cd src-tauri && cargo test`: run Rust unit tests (mainly PM3 parser/builder modules).
- `pwsh ./scripts/check.ps1` (Windows): sequential TS check, Rust check, and Vite build.

## Coding Style & Naming Conventions
- TypeScript: 2-space indentation, semicolons, single quotes, strict typing enabled in `tsconfig.json`.
- React naming: components in `PascalCase.tsx`, hooks in `useX.ts(x)`, utilities in `camelCase.ts`.
- Rust: follow `rustfmt` defaults (4 spaces), modules/files in `snake_case`, keep command handlers in `src-tauri/src/commands/`.
- Keep frontend state changes aligned with backend wizard transitions to preserve dual-FSM sync.

## Testing Guidelines
- Primary automated tests live in Rust modules using `#[test]` (notably PM3 parsing/building code).
- Name tests descriptively with `test_*` and cover parser edge cases and command safety paths.
- Before opening a PR, run at least `npm run build`, `cargo check`, and `cargo test`.
- No formal coverage gate is configured; add tests for all backend logic changes.

## Commit & Pull Request Guidelines
- Match existing history style: concise, imperative subject lines (e.g., `Fix ...`, `Add ...`, `Update ...`).
- Keep commits focused by concern (frontend UX, Rust command flow, packaging/resources).
- PRs should include: purpose, impacted areas, verification steps/commands, and screenshots or short recordings for UI changes.
- Link related issues/audit items when applicable and note any platform-specific impacts (macOS vs Windows sidecar/resource behavior).

## Security & Configuration Tips
- Do not commit secrets or local machine paths.
- Verify sidecar/resource mappings in `src-tauri/tauri.conf.json` when changing binaries/firmware layout.
- Validate shell/command inputs defensively in Rust command handlers.
