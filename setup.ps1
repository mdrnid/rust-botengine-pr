# ==============================================================================
# PR.Social Multi-Worker Engine - 1-Click Automated Setup for New Laptop / PC
# ==============================================================================

[CmdletBinding()]
param()

$ErrorActionPreference = "Continue"

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "   PR.SOCIAL ENGINE - AUTOMATED SETUP FOR NEW LAPTOP / PC                       " -ForegroundColor Yellow
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host ""

# 1. Check Google Chrome
Write-Host "[1/4] Mengecek Google Chrome..." -ForegroundColor Cyan
$chromePath = "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
if (!(Test-Path $chromePath)) {
    $chromePath = "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
}

if (Test-Path $chromePath) {
    Write-Host "   [OK] Google Chrome ditemukan di: $chromePath" -ForegroundColor Green
} else {
    Write-Host "   [!] Google Chrome belum terpasang di komputer ini." -ForegroundColor Yellow
    Write-Host "   Sedang mengunduh installer Google Chrome resmi..." -ForegroundColor Cyan
    $installerPath = "$env:TEMP\ChromeSetup.exe"
    try {
        Invoke-WebRequest -Uri "https://dl.google.com/chrome/install/latest/chrome_installer.exe" -OutFile $installerPath
        Write-Host "   Memasang Google Chrome secara otomatis..." -ForegroundColor Cyan
        Start-Process -FilePath $installerPath -Args "/silent /install" -Wait
        Start-Sleep -Seconds 4
        if (Test-Path "$env:ProgramFiles\Google\Chrome\Application\chrome.exe") {
            Write-Host "   [OK] Google Chrome berhasil dipasang!" -ForegroundColor Green
        } else {
            Write-Host "   [!] Silakan pasang Google Chrome manual dari https://google.com/chrome" -ForegroundColor Yellow
        }
    } catch {
        Write-Host "   [!] Gagal mengunduh Chrome otomatis. Silakan unduh manual: https://google.com/chrome" -ForegroundColor Yellow
    }
}

# 2. Check Rust & Cargo
Write-Host ""
Write-Host "[2/4] Mengecek Rust Toolchain (cargo & rustc)..." -ForegroundColor Cyan
$cargoExists = Get-Command cargo -ErrorAction SilentlyContinue

if ($cargoExists) {
    $cargoVer = cargo --version
    Write-Host "   [OK] $cargoVer terdeteksi!" -ForegroundColor Green
} else {
    Write-Host "   [!] Rust belum terpasang di komputer ini." -ForegroundColor Yellow
    Write-Host "   Mengunduh installer Rustup resmi (Windows x64)..." -ForegroundColor Cyan
    $rustupPath = "$env:TEMP\rustup-init.exe"
    try {
        Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupPath
        Write-Host "   Menjalankan installer Rust (ikuti instruksi di layar, tekan 1 lalu Enter)..." -ForegroundColor Yellow
        Start-Process -FilePath $rustupPath -Wait
        
        # Refresh environment variable PATH di sesi saat ini
        $machinePath = [System.Environment]::GetEnvironmentVariable("Path", "Machine")
        $userPath = [System.Environment]::GetEnvironmentVariable("Path", "User")
        $env:Path = "$machinePath;$userPath;$HOME\.cargo\bin"
        Write-Host "   [OK] Rust Toolchain siap digunakan!" -ForegroundColor Green
    } catch {
        Write-Host "   [!] Silakan install Rust secara manual melalui https://rustup.rs/" -ForegroundColor Red
    }
}

# 3. Create Logs Directory
Write-Host ""
Write-Host "[3/4] Menyiapkan direktori proyek..." -ForegroundColor Cyan
$logDir = Join-Path $PSScriptRoot "logs"
if (!(Test-Path $logDir)) {
    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    Write-Host "   [OK] Folder logs dibuat: $logDir" -ForegroundColor Green
} else {
    Write-Host "   [OK] Folder logs sudah siap." -ForegroundColor Green
}

# 4. Compile Engine to Release Binary
Write-Host ""
Write-Host "[4/4] Mengompilasi engine Rust ke mode release..." -ForegroundColor Cyan
try {
    Push-Location $PSScriptRoot
    cargo build --release
    Pop-Location
    Write-Host "   [OK] Kompilasi selesai! Binary siap di target\release\rust-browser-click-engine.exe" -ForegroundColor Green
} catch {
    Write-Host "   [!] Terjadi kendala saat kompilasi: $_" -ForegroundColor Red
}

Write-Host ""
Write-Host "================================================================================" -ForegroundColor Green
Write-Host "   SETUP SELESAI! ENGINE SIAP DIGUNAKAN DI LAPTOP INI                           " -ForegroundColor Green
Write-Host "================================================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Langkah Memulai Bot di Laptop Baru:" -ForegroundColor Yellow
Write-Host "1. Jalankan Dashboard Web Control Center:" -ForegroundColor White
Write-Host "   .\gui.ps1" -ForegroundColor Cyan
Write-Host "2. Buka dashboard di browser (http://127.0.0.1:8888)" -ForegroundColor White
Write-Host "3. Klik tombol 'Buka Chrome' untuk Worker 1 s.d. 8" -ForegroundColor White
Write-Host "4. Login ke PR.Social dan Instagram sekali saja pada masing-masing worker." -ForegroundColor White
Write-Host "5. Jalankan bot: cargo run --release -- batch-follow --workers-per-batch 2 --batches 4 --tasks 50" -ForegroundColor White
Write-Host ""

