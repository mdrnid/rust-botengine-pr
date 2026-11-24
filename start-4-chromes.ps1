# Start 4 Chrome browser instances for parallel multi-worker automation
$chrome = "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
if (!(Test-Path $chrome)) {
    $chrome = "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
}
if (!(Test-Path $chrome)) {
    throw "Chrome executable not found. Edit `$chrome path in this script."
}

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "   Starting 4 Chrome Instances (Tiled 2x2 Grid)           " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$workers = @(
    @{ Id = 1; Port = 9222; X = 0;   Y = 0;   W = 955; H = 505 },
    @{ Id = 2; Port = 9223; X = 960; Y = 0;   W = 955; H = 505 },
    @{ Id = 3; Port = 9224; X = 0;   Y = 515; W = 955; H = 505 },
    @{ Id = 4; Port = 9225; X = 960; Y = 515; W = 955; H = 505 }
)

foreach ($w in $workers) {
    $id = $w.Id
    $port = $w.Port
    $profile = "$HOME\.browser-automation-profile-$id"
    $pos = "$($w.X),$($w.Y)"
    $size = "$($w.W),$($w.H)"

    Write-Host "Launching Worker #$id -> Port: $port | Profile: $profile" -ForegroundColor Green

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
Write-Host "All 4 Chrome windows started successfully!" -ForegroundColor Yellow
Write-Host "Petunjuk:" -ForegroundColor White
Write-Host "1. Pastikan login ke akun PR.Social dan Instagram di masing-masing jendela browser." -ForegroundColor White
Write-Host "2. Untuk menjalankan 4 worker secara paralel, jalankan:" -ForegroundColor White
Write-Host "   cargo run --release -- auto-follow --workers 4" -ForegroundColor Cyan
Write-Host ""

