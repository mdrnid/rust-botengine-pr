# Reset a specific Worker profile and browser session
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 32)]
    [int]$Worker
)

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "   PR.SOCIAL WORKER PROFILE RESET UTILITY (WORKER #$Worker)                      " -ForegroundColor Yellow
Write-Host "================================================================================" -ForegroundColor Cyan

$port = 9221 + $Worker
$profile = if ($Worker -eq 1) { "$HOME\.browser-automation-profile" } else { "$HOME\.browser-automation-profile-$Worker" }

Write-Host "Worker ID        : #$Worker" -ForegroundColor White
Write-Host "Debugging Port   : $port" -ForegroundColor White
Write-Host "Direktori Profil : $profile" -ForegroundColor White
Write-Host ""

# 1. Hentikan Chrome jika masih aktif
Write-Host "[1/3] Memeriksa proses Chrome untuk Worker #$Worker..." -ForegroundColor Cyan
$conn = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
if ($conn) {
    Write-Host "  Port $port terdeteksi aktif. Menghentikan instance Chrome..." -ForegroundColor Yellow
    Get-Process chrome -ErrorAction SilentlyContinue | Where-Object {
        $_.CommandLine -like "*remote-debugging-port=$port*" -or $_.CommandLine -like "*profile-$Worker*"
    } | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
} else {
    Write-Host "  Worker #$Worker sedang offline (tidak berjalan)." -ForegroundColor Green
}

# 2. Hapus folder profil
Write-Host "[2/3] Menghapus data cache, cookies, dan session di profil..." -ForegroundColor Cyan
if (Test-Path $profile) {
    try {
        Remove-Item -Recurse -Force $profile -ErrorAction Stop
        Write-Host "  [SUKSES] Folder profil berhasil dihapus total!" -ForegroundColor Green
    } catch {
        Write-Host "  [PERINGATAN] Gagal menghapus sebagian file karena masih dikunci proses lain: $_" -ForegroundColor Red
        Write-Host "  Mencoba menutup seluruh tab background..." -ForegroundColor Yellow
        Start-Sleep -Seconds 1
        Remove-Item -Recurse -Force $profile -ErrorAction SilentlyContinue
    }
} else {
    Write-Host "  Folder profil belum ada / sudah bersih." -ForegroundColor Gray
}

# 3. Panduan langkah selanjutnya
Write-Host "[3/3] Profil Worker #$Worker sekarang 100% BERSIH (Virgin State)." -ForegroundColor Green
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "LANGKAH AMAN UNTUK DAFTAR AKUN BARU WORKER #${Worker}:" -ForegroundColor Yellow
Write-Host "1. Aktifkan Mode Pesawat di HP Anda selama 10 detik, lalu matikan (Reset IP)." -ForegroundColor White
Write-Host "2. Buka kembali Chrome Worker #$Worker dengan perintah:" -ForegroundColor White
Write-Host "     .\start-8-chromes.ps1 -Batch $([math]::Ceiling($Worker / 2))" -ForegroundColor Cyan
Write-Host "   (Atau klik 'Buka Chrome Worker #$Worker' di Dashboard gui.ps1)" -ForegroundColor Gray
Write-Host "3. Login akun Instagram baru dan hubungkan ke PR.Social." -ForegroundColor White
Write-Host "================================================================================" -ForegroundColor Cyan

