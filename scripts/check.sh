#!/usr/bin/env bash
# macOS/Linux counterpart of check.ps1.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "[1/4] Frontend tests..."
npm test

echo "[2/4] TypeScript type check..."
npx tsc --noEmit

echo "[3/4] Vite build..."
npx vite build

echo "[4/4] Rust tests..."
(cd src-tauri && cargo test)

echo "[OK] All checks passed"
