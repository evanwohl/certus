# Certus Demo Setup Script (PowerShell)

# Get the script directory and navigate to demo root
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$DemoRoot = Split-Path -Parent $ScriptDir

Write-Host "              CERTUS DEMO SETUP                          " -ForegroundColor Cyan
Write-Host ""

# Check prerequisites
Write-Host "Checking prerequisites..." -ForegroundColor Yellow

try {
    $nodeVersion = node --version
    Write-Host "[OK] Node.js $nodeVersion" -ForegroundColor Green
} catch {
    Write-Host "[ERROR] Node.js not found. Please install Node.js 18+" -ForegroundColor Red
    exit 1
}

try {
    $npmVersion = npm --version
    Write-Host "[OK] npm $npmVersion" -ForegroundColor Green
} catch {
    Write-Host "[ERROR] npm not found. Please install npm" -ForegroundColor Red
    exit 1
}

try {
    $cargoVersion = cargo --version
    Write-Host "[OK] $cargoVersion" -ForegroundColor Green
} catch {
    Write-Host "[ERROR] Cargo not found. Please install Rust: https://rustup.rs" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Installing Node.js dependencies..." -ForegroundColor Yellow
Set-Location $DemoRoot
npm install

Write-Host ""
Write-Host "Installing frontend dependencies..." -ForegroundColor Yellow
Set-Location "$DemoRoot\frontend"
npm install

Write-Host ""
Write-Host "Building python-verifier library..." -ForegroundColor Yellow
Set-Location "$DemoRoot\..\python-verifier"
cargo build --release

Write-Host ""
Write-Host "Building python-cli..." -ForegroundColor Yellow
Set-Location "$DemoRoot\python-cli"
cargo build --release

Write-Host ""
Write-Host "[SUCCESS] Setup complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Launch the demo:" -ForegroundColor Cyan
Write-Host "   cd $DemoRoot" -ForegroundColor White
Write-Host "   npm run demo" -ForegroundColor White
Write-Host ""
Write-Host "Then open: http://localhost:3000" -ForegroundColor Cyan
Write-Host ""

# Return to demo root
Set-Location $DemoRoot
