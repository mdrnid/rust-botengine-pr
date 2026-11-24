# Start 2 Chrome browser instances side-by-side (Left & Right)
$chrome = "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
if (!(Test-Path $chrome)) {
    $chrome = "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
}
if (!(Test-Path $chrome)) {
    throw "Chrome executable not found. Edit `$chrome path in this script."
}

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "   Starting 2 Chrome Instances (Side-by-Side: Left/Right)  " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$workers = @(
    @{ Id = 1; Port = 9222; Profile = "$HOME\.browser-automation-profile";   X = 0;   Y = 0; W = 765; H = 820 },
    @{ Id = 2; Port = 9223; Profile = "$HOME\.browser-automation-profile-2"; X = 770; Y = 0; W = 765; H = 820 }
)

foreach ($w in $workers) {
    $id = $w.Id
    $port = $w.Port
    $profile = $w.Profile
    $pos = "$($w.X),$($w.Y)"
    $size = "$($w.W),$($w.H)"

    # Check if this port is already listening so we don't open duplicates
    $alreadyRunning = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
    if ($alreadyRunning) {
        Write-Host "Worker #$id (Port $port) is ALREADY RUNNING. Skipping launch." -ForegroundColor Yellow
        continue
    }

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

    Start-Sleep -Milliseconds 800
}

Write-Host ""
Write-Host "2 Chrome windows ready!" -ForegroundColor Yellow
Write-Host "Jalankan bot dengan:" -ForegroundColor White
Write-Host "   cargo run --release -- auto-follow --workers 2 --fast" -ForegroundColor Cyan
Write-Host ""

