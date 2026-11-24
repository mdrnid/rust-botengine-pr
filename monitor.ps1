# Live Monitor for PR.Social Multi-Worker Automation Engine
param(
    [switch]$Watch,
    [int]$Interval = 3
)

function Show-Dashboard {
    Clear-Host
    $now = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    Write-Host "================================================================================" -ForegroundColor Cyan
    Write-Host "            PR.SOCIAL MULTI-WORKER ENGINE MONITOR - $now                        " -ForegroundColor Yellow
    Write-Host "================================================================================" -ForegroundColor Cyan

    # 1. Engine Process Status
    $engineProcesses = Get-Process -Name "rust-browser-click-engine" -ErrorAction SilentlyContinue
    if ($engineProcesses) {
        $pids = ($engineProcesses | ForEach-Object { "$($_.Id)" }) -join ", "
        $memMb = [math]::Round((($engineProcesses | Measure-Object -Property WorkingSet64 -Sum).Sum / 1MB), 1)
        Write-Host " Engine Status : " -NoNewline
        Write-Host "RUNNING" -ForegroundColor Green -NoNewline
        Write-Host " (PID: $pids | Memory: ${memMb} MB)" -ForegroundColor Gray
    } else {
        Write-Host " Engine Status : " -NoNewline
        Write-Host "STOPPED" -ForegroundColor DarkGray -NoNewline
        Write-Host " (Belum dijalankan)" -ForegroundColor DarkGray
    }
    Write-Host "--------------------------------------------------------------------------------" -ForegroundColor DarkGray

    # 2. Worker Endpoints (Ports 9222..9229 across 4 Batches)
    Write-Host " WORKER BROWSER INSTANCES (Chrome Debugging Endpoints across 4 Batches):" -ForegroundColor White
    $ports = @(9222..9229)
    $onlineCount = 0

    foreach ($port in $ports) {
        $wid = $port - 9221
        $batchId = [math]::Ceiling($wid / 2)
        $online = $false
        $prFound = "Missing"
        $igFound = "Missing"
        $pageCount = 0
        $activeTitle = "-"

        # Test both 127.0.0.1 and [::1]
        $endpoints = @("http://127.0.0.1:$port", "http://[::1]:$port")
        foreach ($ep in $endpoints) {
            try {
                $resp = Invoke-RestMethod -Uri "$ep/json/list" -Method Get -TimeoutSec 1 -ErrorAction Stop
                $online = $true
                $pageCount = ($resp | Where-Object { $_.type -eq "page" }).Count
                $prTab = $resp | Where-Object { $_.url -like "*pr.social*" } | Select-Object -First 1
                $igTab = $resp | Where-Object { $_.url -like "*instagram.com*" } | Select-Object -First 1

                if ($prTab) { $prFound = "Ready" }
                if ($igTab) { $igFound = "Ready" }
                if ($prTab) { $activeTitle = $prTab.title }
                break
            } catch {
                # try next endpoint
            }
        }

        if ($online) {
            $onlineCount++
            Write-Host "  [Batch #$batchId] Worker #$wid (Port $port): " -NoNewline
            Write-Host "[ONLINE] " -ForegroundColor Green -NoNewline
            Write-Host "Tabs: $pageCount | PR.Social: " -NoNewline
            if ($prFound -eq "Ready") { Write-Host "OK " -ForegroundColor Green -NoNewline } else { Write-Host "MISSING " -ForegroundColor Red -NoNewline }
            Write-Host "| Instagram: " -NoNewline
            if ($igFound -eq "Ready") { Write-Host "OK" -ForegroundColor Green } else { Write-Host "MISSING" -ForegroundColor Red }
        } else {
            Write-Host "  [Batch #$batchId] Worker #$wid (Port $port): " -NoNewline
            Write-Host "[OFFLINE]" -ForegroundColor Red -NoNewline
            Write-Host " (Belum dibuka)" -ForegroundColor DarkGray
        }
    }

    Write-Host "--------------------------------------------------------------------------------" -ForegroundColor DarkGray

    # 3. Recent Log Activity (from logs/ directory)
    Write-Host " RECENT TASK ACTIVITY (Logs):" -ForegroundColor White
    $logDir = Join-Path $PSScriptRoot "logs"
    if (Test-Path $logDir) {
        $logFiles = Get-ChildItem -Path $logDir -Filter "worker*.jsonl" | Sort-Object LastWriteTime -Descending | Select-Object -First 4
        if ($logFiles) {
            foreach ($file in $logFiles) {
                $lines = Get-Content -Path $file.FullName -ErrorAction SilentlyContinue
                if ($lines) {
                    $totalTasks = ($lines | Where-Object { $_ -match '"task_number"' }).Count
                    $lastLine = $lines[-1]
                    try {
                        $parsed = $lastLine | ConvertFrom-Json
                        $statusColor = if ($parsed.status -eq "success") { "Green" } else { "Yellow" }
                        $wId = $parsed.worker_id
                        $tNum = $parsed.task_number
                        $st = $parsed.status
                        $time = $parsed.timestamp.Substring(11, 8)
                        $url = $parsed.instagram_url
                        if ($url.Length -gt 38) { $url = $url.Substring(0, 35) + "..." }

                        Write-Host "  [$time] Worker #$wId " -ForegroundColor Cyan -NoNewline
                        Write-Host "- Task #$tNum [$st] " -ForegroundColor $statusColor -NoNewline
                        Write-Host "-> $url (Total Selesai: $totalTasks)" -ForegroundColor Gray
                    } catch {
                        Write-Host "  $($file.Name): $totalTasks tasks recorded" -ForegroundColor Gray
                    }
                }
            }
        } else {
            Write-Host "  (Belum ada file log di folder logs/)" -ForegroundColor DarkGray
        }
    } else {
        Write-Host "  (Folder logs/ belum dibuat)" -ForegroundColor DarkGray
    }

    Write-Host "================================================================================" -ForegroundColor Cyan
    Write-Host " Active Workers Online: $onlineCount / 8" -ForegroundColor Yellow
    if ($Watch) {
        Write-Host " Auto-refreshing every $Interval seconds. Press Ctrl+C to exit." -ForegroundColor DarkGray
    }
}

# Run loop or one-shot
if ($Watch) {
    while ($true) {
        Show-Dashboard
        Start-Sleep -Seconds $Interval
    }
} else {
    Show-Dashboard
}

