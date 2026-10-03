Write-Host "[1/4] Frontend tests..." -ForegroundColor Cyan
npm test
if ($LASTEXITCODE -ne 0) { Write-Host "[FAIL] Frontend tests" -ForegroundColor Red; exit 1 }

Write-Host "[2/4] TypeScript type check..." -ForegroundColor Cyan
npx tsc --noEmit
if ($LASTEXITCODE -ne 0) { Write-Host "[FAIL] TypeScript" -ForegroundColor Red; exit 1 }

Write-Host "[3/4] Vite build..." -ForegroundColor Cyan
npx vite build
if ($LASTEXITCODE -ne 0) { Write-Host "[FAIL] Vite" -ForegroundColor Red; exit 1 }

Write-Host "[4/4] Rust tests..." -ForegroundColor Cyan
Push-Location src-tauri
cargo test 2>&1
$rc = $LASTEXITCODE
Pop-Location
if ($rc -ne 0) { Write-Host "[FAIL] Rust" -ForegroundColor Red; exit 1 }

Write-Host "[OK] All checks passed" -ForegroundColor Green
