# Start 8 Chrome browser instances organized into 4 Batches (2 Workers per Batch)
param(
    [ValidateSet("1", "2", "3", "4", "All")]
    [string]$Batch = "All"
)

$chrome = "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
if (!(Test-Path $chrome)) {
    $chrome = "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
}
if (!(Test-Path $chrome)) {
    throw "Chrome executable not found. Edit `$chrome path in this script."
}

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "   PR.SOCIAL 8-WORKER CHROME LAUNCHER (4 BATCHES)                               " -ForegroundColor Yellow
Write-Host "================================================================================" -ForegroundColor Cyan

$workers = @(
    # Batch 1 (Workers 1 & 2)
    @{ Id = 1; Batch = 1; Port = 9222; Profile = "$HOME\.browser-automation-profile";   X = 0;   Y = 0; W = 765; H = 820 },
    @{ Id = 2; Batch = 1; Port = 9223; Profile = "$HOME\.browser-automation-profile-2"; X = 770; Y = 0; W = 765; H = 820 },

    # Batch 2 (Workers 3 & 4)
    @{ Id = 3; Batch = 2; Port = 9224; Profile = "$HOME\.browser-automation-profile-3"; X = 0;   Y = 0; W = 765; H = 820 },
    @{ Id = 4; Batch = 2; Port = 9225; Profile = "$HOME\.browser-automation-profile-4"; X = 770; Y = 0; W = 765; H = 820 },

    # Batch 3 (Workers 5 & 6)
    @{ Id = 5; Batch = 3; Port = 9226; Profile = "$HOME\.browser-automation-profile-5"; X = 0;   Y = 0; W = 765; H = 820 },
    @{ Id = 6; Batch = 3; Port = 9227; Profile = "$HOME\.browser-automation-profile-6"; X = 770; Y = 0; W = 765; H = 820 },

    # Batch 4 (Workers 7 & 8)
    @{ Id = 7; Batch = 4; Port = 9228; Profile = "$HOME\.browser-automation-profile-7"; X = 0;   Y = 0; W = 765; H = 820 },
    @{ Id = 8; Batch = 4; Port = 9229; Profile = "$HOME\.browser-automation-profile-8"; X = 770; Y = 0; W = 765; H = 820 }
)

$targetWorkers = if ($Batch -eq "All") {
    $workers
} else {
    $batchNum = [int]$Batch
    $workers | Where-Object { $_.Batch -eq $batchNum }
}

Write-Host "Target Target Peluncuran: Batch $Batch ($($targetWorkers.Count) Worker)" -ForegroundColor White
Write-Host ""

foreach ($w in $targetWorkers) {
    $id = $w.Id
    $bId = $w.Batch
    $port = $w.Port
    $profile = $w.Profile
    $pos = "$($w.X),$($w.Y)"
    $size = "$($w.W),$($w.H)"

    # Cek apakah port sudah aktif
    $alreadyRunning = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
    if ($alreadyRunning) {
        Write-Host "  [Batch #$bId] Worker #$id (Port $port) SUDAH BERJALAN. Melewati..." -ForegroundColor Yellow
        continue
    }

    Write-Host "  [Batch #$bId] Meluncurkan Worker #$id -> Port: $port | Profil: $profile" -ForegroundColor Green

    Start-Process -FilePath $chrome -ArgumentList @(
        "--remote-debugging-port=$port",
        "--remote-debugging-address=127.0.0.1",
        "--user-data-dir=`"$profile`"",
        "--window-position=$pos",
        "--window-size=$size",
        "--disable-blink-features=AutomationControlled",
        "--disable-features=IsolateOrigins,site-per-process",
        "--disable-infobars",
        "--flag-switches-begin", "--flag-switches-end",
        "--disable-background-timer-throttling",
        "--disable-backgrounding-occluded-windows",
        "--disable-renderer-backgrounding",
        "--no-first-run",
        "--no-default-browser-check",
        "https://www.pr.social/tasks"
    )

    Start-Sleep -Milliseconds 600
}

Write-Host ""
Write-Host "Browser selesai diluncurkan!" -ForegroundColor Yellow
Write-Host "Petunjuk:" -ForegroundColor White
Write-Host "1. Pastikan setiap jendela yang baru pertama kali dibuka sudah login ke akun PR.Social & Instagram." -ForegroundColor Gray
Write-Host "2. Untuk menjalankan sistem rotasi 4 batch, gunakan:" -ForegroundColor White
Write-Host "   cargo run --release -- batch-follow" -ForegroundColor Cyan
Write-Host ""

