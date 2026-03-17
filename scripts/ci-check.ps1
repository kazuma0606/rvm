# CI equivalent checks for MVP completion gate

$ErrorActionPreference = "Stop"

Write-Host "=== RVM / ForgeScript MVP Completion Gate ===" -ForegroundColor Cyan
Write-Host ""

# 1. Workspace check
Write-Host "[1/5] Running cargo check --workspace..." -ForegroundColor Yellow
cargo check --workspace
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Workspace check failed" -ForegroundColor Red
    exit 1
}
Write-Host "✅ Workspace check passed" -ForegroundColor Green
Write-Host ""

# 2. All tests
Write-Host "[2/5] Running cargo test --workspace..." -ForegroundColor Yellow
cargo test --workspace --quiet
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Tests failed" -ForegroundColor Red
    exit 1
}
Write-Host "✅ All tests passed" -ForegroundColor Green
Write-Host ""

# 3. Build CLI
Write-Host "[3/5] Building forge CLI..." -ForegroundColor Yellow
cargo build --release -p fs-cli
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ CLI build failed" -ForegroundColor Red
    exit 1
}
Write-Host "✅ CLI build successful" -ForegroundColor Green
Write-Host ""

# 4. E2E tests
Write-Host "[4/5] Running E2E tests..." -ForegroundColor Yellow
cargo test -p e2e-tests
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ E2E tests failed" -ForegroundColor Red
    exit 1
}
Write-Host "✅ E2E tests passed" -ForegroundColor Green
Write-Host ""

# 5. Manual fixture run
Write-Host "[5/5] Running manual fixture test..." -ForegroundColor Yellow
$forgeExe = ".\target\release\forge.exe"
& $forgeExe run fixtures\e2e\arithmetic.fs
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Manual fixture run failed" -ForegroundColor Red
    exit 1
}
Write-Host "✅ Manual fixture run successful" -ForegroundColor Green
Write-Host ""

Write-Host "=== 🎉 All MVP Gate Checks Passed! 🎉 ===" -ForegroundColor Green
