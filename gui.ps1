# Lightweight Web Management Interface for PR.Social Multi-Worker Engine
# Runs a zero-dependency local web server on http://127.0.0.1:8888

$port = 8888
$url = "http://127.0.0.1:$port/"
$chromePath = "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
if (!(Test-Path $chromePath)) {
    $chromePath = "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
}

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "      PR.SOCIAL MULTI-WORKER WEB CONTROL CENTER                                 " -ForegroundColor Yellow
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "Menjalankan local dashboard di: $url" -ForegroundColor Green
Write-Host "Tekan Ctrl+C di terminal ini untuk mematikan control center." -ForegroundColor Gray
Write-Host ""

$listener = New-Object System.Net.HttpListener
$listener.Prefixes.Add($url)
try {
    $listener.Start()
} catch {
    Write-Host "Gagal membuka port $port. Kemungkinan port sedang digunakan: $_" -ForegroundColor Red
    exit 1
}

# Auto-open browser
Start-Process $url

function Get-StatusJson {
    $engineProc = Get-Process -Name "rust-browser-click-engine" -ErrorAction SilentlyContinue
    $engineRunning = $false
    $enginePid = $null
    if ($engineProc) {
        $engineRunning = $true
        $enginePid = $engineProc.Id
    }

    $workerList = @()
    # 1. Instant Port Scan: Dapatkan semua listening port lokal sekaligus (~5-10ms)
    $activePorts = [System.Collections.Generic.HashSet[int]]::new()
    try {
        $conns = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue
        foreach ($c in $conns) {
            $null = $activePorts.Add([int]$c.LocalPort)
        }
    } catch {}

    $maxPort = 9229
    foreach ($p in 9230..9253) {
        if ($activePorts.Contains($p)) {
            $maxPort = [math]::Max($maxPort, $p)
        }
    }
    $ports = @(9222..$maxPort)

    foreach ($p in $ports) {
        $wid = $p - 9221
        $batchId = [math]::Ceiling($wid / 2)
        $online = $false
        $prFound = $false
        $igFound = $false
        $pageCount = 0

        # Hanya query HTTP jika port terbukti AKTIF (menghindari timeout lambat pada port mati)
        if ($activePorts.Contains($p)) {
            $online = $true
            try {
                $resp = Invoke-RestMethod -Uri "http://127.0.0.1:$p/json/list" -Method Get -TimeoutSec 1 -ErrorAction SilentlyContinue
                if ($resp) {
                    $pageCount = ($resp | Where-Object { $_.type -eq "page" }).Count
                    $prTab = $resp | Where-Object { $_.url -like "*pr.social*" } | Select-Object -First 1
                    $igTab = $resp | Where-Object { $_.url -like "*instagram.com*" } | Select-Object -First 1
                    if ($prTab) { $prFound = $true }
                    if ($igTab) { $igFound = $true }
                }
            } catch {}
        }

        $workerList += @{
            id = $wid
            batch = $batchId
            port = $p
            online = $online
            pageCount = $pageCount
            prFound = $prFound
            igFound = $igFound
        }
    }

    # Logs: baca hanya baris terakhir dari 6 log file terbaru
    $recentLogs = @()
    $logDir = Join-Path $PSScriptRoot "logs"
    if (Test-Path $logDir) {
        $logFiles = Get-ChildItem -Path $logDir -Filter "worker*.jsonl" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 6
        foreach ($f in $logFiles) {
            $last = Get-Content -Path $f.FullName -Tail 1 -ErrorAction SilentlyContinue
            if ($last) {
                try {
                    $json = $last | ConvertFrom-Json
                    $recentLogs += @{
                        file = $f.Name
                        time = $json.timestamp
                        worker_id = $json.worker_id
                        task_number = $json.task_number
                        url = $json.instagram_url
                        status = $json.status
                        duration_ms = $json.duration_ms
                    }
                } catch {}
            }
        }
    }

    $data = @{
        engineRunning = $engineRunning
        enginePid = $enginePid
        workers = $workerList
        recentLogs = $recentLogs
    }

    return ($data | ConvertTo-Json -Depth 4)
}

function Launch-WorkerChrome([int]$workerId) {
    $batch = [math]::Ceiling($workerId / 2)
    $port = 9221 + $workerId
    $profile = if ($workerId -eq 1) { "$HOME\.browser-automation-profile" } else { "$HOME\.browser-automation-profile-$workerId" }

    $running = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
    if ($running) {
        return @{ success = $false; message = "Worker #$workerId (Port $port) sudah aktif." }
    }

    Start-Process -FilePath $chromePath -ArgumentList @(
        "--remote-debugging-port=$port",
        "--remote-debugging-address=127.0.0.1",
        "--user-data-dir=`"$profile`"",
        "--disable-blink-features=AutomationControlled",
        "--disable-features=IsolateOrigins,site-per-process",
        "--disable-infobars",
        "--flag-switches-begin", "--flag-switches-end",
        "--disable-background-timer-throttling",
        "--disable-backgrounding-occluded-windows",
        "--disable-renderer-backgrounding",
        "--no-first-run",
        "--no-default-browser-check",
        "--js-flags=--max-old-space-size=256",
        "--disk-cache-size=10485760",
        "--media-cache-size=10485760",
        "--disable-extensions",
        "--disable-component-update",
        "--disable-sync",
        "--disable-default-apps",
        "--mute-audio",
        "--renderer-process-limit=2",
        "--disable-speech-api",
        "--disable-breakpad",
        "--disable-domain-reliability",
        "https://www.pr.social/tasks"
    )

    return @{ success = $true; message = "Worker #$workerId berhasil diluncurkan pada Port $port (Profil: $profile)." }
}

function Reset-WorkerProfile([int]$workerId) {
    $port = 9221 + $workerId
    $profile = if ($workerId -eq 1) { "$HOME\.browser-automation-profile" } else { "$HOME\.browser-automation-profile-$workerId" }

    Get-Process chrome -ErrorAction SilentlyContinue | Where-Object {
        $_.CommandLine -like "*remote-debugging-port=$port*" -or $_.CommandLine -like "*profile-$workerId*"
    } | Stop-Process -Force -ErrorAction SilentlyContinue

    Start-Sleep -Milliseconds 500

    if (Test-Path $profile) {
        try {
            Remove-Item -Recurse -Force $profile -ErrorAction Stop
            return @{ success = $true; message = "Profil Worker #$workerId ($profile) berhasil dihapus total. Profil siap digunakan untuk akun baru!" }
        } catch {
            return @{ success = $false; message = "Gagal menghapus folder profil: $_" }
        }
    } else {
        return @{ success = $true; message = "Profil Worker #$workerId belum ada (sudah bersih)." }
    }
}

$htmlPage = @'
<!DOCTYPE html>
<html lang="id">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>PR.Social Worker Control Center</title>
    <style>
        :root {
            --bg: #0b0f19;
            --card-bg: #111827;
            --border: #1f2937;
            --text-main: #f3f4f6;
            --text-muted: #9ca3af;
            --primary: #38bdf8;
            --primary-hover: #0284c7;
            --green: #10b981;
            --red: #ef4444;
            --yellow: #f59e0b;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: var(--bg);
            color: var(--text-main);
            padding: 24px;
            line-height: 1.5;
        }
        .container { max-width: 1200px; margin: 0 auto; }
        header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            background: var(--card-bg);
            padding: 20px 24px;
            border-radius: 12px;
            border: 1px solid var(--border);
            margin-bottom: 24px;
        }
        .title h1 { font-size: 20px; font-weight: 700; color: #fff; }
        .title p { font-size: 13px; color: var(--text-muted); }
        .status-badge {
            display: inline-flex;
            align-items: center;
            gap: 8px;
            padding: 6px 14px;
            border-radius: 999px;
            font-size: 13px;
            font-weight: 600;
        }
        .status-badge.online { background: rgba(16, 185, 129, 0.15); color: var(--green); border: 1px solid rgba(16, 185, 129, 0.3); }
        .status-badge.offline { background: rgba(239, 68, 68, 0.15); color: var(--red); border: 1px solid rgba(239, 68, 68, 0.3); }
        .btn-group { display: flex; gap: 12px; margin-bottom: 24px; flex-wrap: wrap; }
        button {
            background: var(--card-bg);
            color: var(--text-main);
            border: 1px solid var(--border);
            padding: 10px 18px;
            border-radius: 8px;
            font-size: 14px;
            font-weight: 600;
            cursor: pointer;
            transition: all 0.2s;
            display: inline-flex;
            align-items: center;
            gap: 8px;
        }
        button:hover { background: #1e293b; border-color: var(--primary); }
        button.btn-primary { background: var(--primary); color: #000; border: none; }
        button.btn-primary:hover { background: var(--primary-hover); }
        button.btn-danger { background: rgba(239, 68, 68, 0.2); color: var(--red); border-color: rgba(239, 68, 68, 0.4); }
        button.btn-danger:hover { background: var(--red); color: #fff; }
        .config-card {
            background: var(--card-bg);
            border: 1px solid var(--border);
            border-radius: 12px;
            padding: 18px 22px;
            margin-bottom: 20px;
        }
        .config-header {
            display: flex;
            align-items: center;
            gap: 10px;
            margin-bottom: 14px;
        }
        .config-header h2 {
            font-size: 15px;
            font-weight: 700;
            color: #fff;
            margin: 0;
        }
        .config-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
            gap: 12px;
            align-items: end;
        }
        .form-group {
            display: flex;
            flex-direction: column;
            gap: 6px;
        }
        .form-group label {
            font-size: 12px;
            font-weight: 600;
            color: var(--text-muted);
        }
        .form-group select, .form-group input {
            background: #0b0f19;
            border: 1px solid var(--border);
            color: #fff;
            padding: 9px 12px;
            border-radius: 8px;
            font-size: 13px;
            outline: none;
            transition: border-color 0.2s;
        }
        .form-group select:focus, .form-group input:focus {
            border-color: var(--primary);
        }
        .btn-custom-launch {
            background: var(--primary);
            color: #000;
            font-weight: 700;
            padding: 10px 18px;
            border: none;
            border-radius: 8px;
            cursor: pointer;
            width: 100%;
            justify-content: center;
            display: inline-flex;
            align-items: center;
            gap: 6px;
            font-size: 13px;
        }
        .btn-custom-launch:hover {
            background: var(--primary-hover);
        }
        .batch-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(270px, 1fr));
            gap: 18px;
            margin-bottom: 24px;
        }
        .batch-card {
            background: var(--card-bg);
            border: 1px solid var(--border);
            border-radius: 12px;
            padding: 18px;
        }
        .batch-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            border-bottom: 1px solid var(--border);
            padding-bottom: 12px;
            margin-bottom: 14px;
        }
        .batch-title { font-size: 15px; font-weight: 700; color: var(--primary); }
        .worker-item {
            background: #0f172a;
            border: 1px solid #1e293b;
            border-radius: 8px;
            padding: 12px;
            margin-bottom: 10px;
        }
        .worker-item:last-child { margin-bottom: 0; }
        .worker-top { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
        .worker-name { font-size: 14px; font-weight: 600; }
        .pill { font-size: 11px; padding: 2px 8px; border-radius: 99px; font-weight: 600; }
        .pill.green { background: rgba(16, 185, 129, 0.2); color: var(--green); }
        .pill.red { background: rgba(239, 68, 68, 0.2); color: var(--red); }
        .worker-meta { font-size: 12px; color: var(--text-muted); margin-bottom: 8px; }
        .worker-tabs { display: flex; gap: 6px; font-size: 11px; margin-bottom: 10px; }
        .tab-tag { padding: 2px 6px; border-radius: 4px; }
        .tab-tag.ok { background: rgba(16, 185, 129, 0.2); color: var(--green); }
        .tab-tag.missing { background: rgba(239, 68, 68, 0.2); color: var(--red); }
        .worker-actions { display: flex; gap: 6px; }
        .btn-sm { padding: 6px 10px; font-size: 12px; justify-content: center; flex: 1; }
        .btn-icon { flex: 0 0 auto; padding: 6px 10px; }
        .logs-panel {
            background: var(--card-bg);
            border: 1px solid var(--border);
            border-radius: 12px;
            padding: 20px;
        }
        .logs-panel h2 { font-size: 16px; margin-bottom: 14px; color: #fff; }
        .log-item {
            display: flex;
            align-items: center;
            justify-content: space-between;
            font-size: 13px;
            padding: 8px 12px;
            border-bottom: 1px solid #1e293b;
        }
        .log-item:last-child { border-bottom: none; }
        .log-time { color: var(--text-muted); font-family: monospace; }
        .log-url { color: var(--primary); text-decoration: none; max-width: 380px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div class="title">
                <h1>PR.Social Worker Control Center</h1>
                <p>Dashboard Ringan Manajemen & Pendaftaran Worker Browser</p>
            </div>
            <div id="engineStatus" class="status-badge offline">
                <span>&#9679;</span> Engine: STOPPED
            </div>
        </header>

        <div class="config-card">
            <div class="config-header">
                <span style="font-size: 18px;">⚙️</span>
                <h2>Peluncuran Bot Kustom (Atur Jumlah Batch & Worker)</h2>
            </div>
            <div class="config-grid">
                <div class="form-group">
                    <label for="cfgWorkers">Worker per Batch</label>
                    <select id="cfgWorkers">
                        <option value="1">1 Worker (Single)</option>
                        <option value="2" selected>2 Worker (Kiri & Kanan)</option>
                        <option value="3">3 Worker (Grid 3)</option>
                        <option value="4">4 Worker (Grid 2x2)</option>
                        <option value="5">5 Worker (Grid 5)</option>
                        <option value="6">6 Worker (Grid 3x2)</option>
                        <option value="7">7 Worker (Grid 7)</option>
                        <option value="8">8 Worker (Grid 4x2)</option>
                    </select>
                </div>
                <div class="form-group">
                    <label for="cfgTasks">Target Task / Worker</label>
                    <input type="number" id="cfgTasks" value="50" min="5" max="200" step="5" />
                </div>
                <div class="form-group">
                    <label for="cfgBrowserMode">Manajemen Browser</label>
                    <select id="cfgBrowserMode">
                        <option value="auto" selected>Auto Buka-Tutup (Hemat RAM)</option>
                        <option value="keep">Tetap Buka di Background</option>
                    </select>
                </div>
                <div class="form-group">
                    <button class="btn-custom-launch" onclick="launchCustomBatch()">
                        &#128640; Mulai Bot Kustom
                    </button>
                </div>
                <div class="form-group" style="grid-column: 1 / -1; background: #070c16; padding: 12px 14px; border-radius: 8px; border: 1px solid var(--border); margin-top: 4px;">
                    <label style="color: var(--primary); font-weight: 700; font-size: 13px;">
                        Pilih Batch Aktif (Centang untuk Menjalankan, Hapus Centang untuk Skip / Lewati):
                    </label>
                    <div style="display: flex; gap: 20px; margin-top: 8px; flex-wrap: wrap;">
                        <label style="display: flex; align-items: center; gap: 8px; cursor: pointer; color: #fff; font-size: 13px;">
                            <input type="checkbox" id="chkBatch1" value="1" checked style="accent-color: var(--primary); width: 16px; height: 16px;">
                            <span><b>Batch 1</b> (Worker 1 & 2)</span>
                        </label>
                        <label style="display: flex; align-items: center; gap: 8px; cursor: pointer; color: #fff; font-size: 13px;">
                            <input type="checkbox" id="chkBatch2" value="2" checked style="accent-color: var(--primary); width: 16px; height: 16px;">
                            <span><b>Batch 2</b> (Worker 3 & 4)</span>
                        </label>
                        <label style="display: flex; align-items: center; gap: 8px; cursor: pointer; color: #fff; font-size: 13px;">
                            <input type="checkbox" id="chkBatch3" value="3" checked style="accent-color: var(--primary); width: 16px; height: 16px;">
                            <span><b>Batch 3</b> (Worker 5 & 6)</span>
                        </label>
                        <label style="display: flex; align-items: center; gap: 8px; cursor: pointer; color: #fff; font-size: 13px;">
                            <input type="checkbox" id="chkBatch4" value="4" checked style="accent-color: var(--primary); width: 16px; height: 16px;">
                            <span><b>Batch 4</b> (Worker 7 & 8)</span>
                        </label>
                    </div>
                </div>
            </div>
        </div>

        <div class="btn-group">
            <button onclick="startCommand('batch-follow --active-batches 1,2,3,4 --tasks 50')">&#128640; Semua 4 Batch (W1..8)</button>
            <button onclick="startCommand('batch-follow --active-batches 2,3,4 --tasks 50')">&#9889; Skip Batch 1 (Batch 2..4)</button>
            <button onclick="startCommand('batch-follow --active-batches 2,3 --tasks 50')">&#9889; Batch 2 & 3 (W3..6)</button>
            <button onclick="startCommand('auto-follow --workers 2 --fast')">&#9889; 2 Worker Fast Standby</button>
            <button onclick="launchAllChromes()">&#128421; Buka Semua 8 Chrome</button>
            <button class="btn-danger" onclick="stopEngine()">&#128721; Hentikan Bot</button>
            <button onclick="refreshData()">&#128260; Refresh</button>
        </div>

        <div class="batch-grid" id="batchContainer">
            <!-- Will be populated by JS -->
        </div>

        <div class="logs-panel">
            <h2>Aktivitas Task Terakhir (Live Logs)</h2>
            <div id="logsContainer">
                <p style="color: var(--text-muted); font-size: 13px;">Memuat riwayat task...</p>
            </div>
        </div>
    </div>

    <script>
        let isFetching = false;
        async function refreshData() {
            if (isFetching) return;
            isFetching = true;
            try {
                const res = await fetch('/api/status');
                if (res.ok) {
                    const data = await res.json();
                    renderUI(data);
                }
            } catch (err) {
                // Ignore transient network errors
            } finally {
                isFetching = false;
            }
        }

        function renderUI(data) {
            // Engine Status
            const badge = document.getElementById('engineStatus');
            if (data.engineRunning) {
                badge.className = 'status-badge online';
                badge.innerHTML = `<span>&#9679;</span> Engine: RUNNING (PID: ${data.enginePid})`;
            } else {
                badge.className = 'status-badge offline';
                badge.innerHTML = '<span>&#9679;</span> Engine: STOPPED';
            }

            // Batches (1..4)
            const container = document.getElementById('batchContainer');
            container.innerHTML = '';

            for (let b = 1; b <= 4; b++) {
                const workersInBatch = data.workers.filter(w => w.batch === b);
                let workersHtml = '';

                workersInBatch.forEach(w => {
                    const statusPill = w.online 
                        ? `<span class="pill green">ONLINE</span>` 
                        : `<span class="pill red">OFFLINE</span>`;

                    const prTag = w.prFound ? `<span class="tab-tag ok">PR.Social: OK</span>` : `<span class="tab-tag missing">PR: Missing</span>`;
                    const igTag = w.igFound ? `<span class="tab-tag ok">IG: OK</span>` : `<span class="tab-tag missing">IG: Missing</span>`;

                    const actionBtn = w.online 
                        ? `<div class="worker-actions">
                             <button class="btn-sm" disabled style="opacity: 0.6;">&#9989; Aktif (${w.port})</button>
                             <button class="btn-sm btn-danger btn-icon" title="Reset & Bersihkan Profil" onclick="resetWorkerProfile(${w.id})">&#128465; Reset</button>
                           </div>` 
                        : `<div class="worker-actions">
                             <button class="btn-sm" onclick="launchWorker(${w.id})">&#127760; Buka Chrome</button>
                             <button class="btn-sm btn-danger btn-icon" title="Reset & Hapus Session Akun" onclick="resetWorkerProfile(${w.id})">&#128465; Reset</button>
                           </div>`;

                    workersHtml += `
                        <div class="worker-item">
                            <div class="worker-top">
                                <span class="worker-name">Worker #${w.id}</span>
                                ${statusPill}
                            </div>
                            <div class="worker-meta">Port: ${w.port} | Profil: profile-${w.id}</div>
                            <div class="worker-tabs">
                                ${prTag} ${igTag}
                            </div>
                            ${actionBtn}
                        </div>
                    `;
                });

                const card = document.createElement('div');
                card.className = 'batch-card';
                card.innerHTML = `
                    <div class="batch-header">
                        <span class="batch-title">BATCH #${b}</span>
                        <span style="font-size: 12px; color: var(--text-muted);">2 Worker (Target 50 Task)</span>
                    </div>
                    ${workersHtml}
                `;
                container.appendChild(card);
            }

            // Logs
            const logsEl = document.getElementById('logsContainer');
            if (data.recentLogs && data.recentLogs.length > 0) {
                logsEl.innerHTML = data.recentLogs.map(l => {
                    const t = l.time ? l.time.substring(11, 19) : '-';
                    const stColor = l.status === 'success' ? 'var(--green)' : 'var(--yellow)';
                    return `
                        <div class="log-item">
                            <span class="log-time">[${t}] Worker #${l.worker_id} - Task #${l.task_number}</span>
                            <span class="log-url">${l.url || '-'}</span>
                            <span style="color: ${stColor}; font-weight: 600;">[${l.status}] (${l.duration_ms}ms)</span>
                        </div>
                    `;
                }).join('');
            } else {
                logsEl.innerHTML = `<p style="color: var(--text-muted); font-size: 13px;">Belum ada aktivitas baru.</p>`;
            }
        }

        async function launchWorker(id) {
            try {
                const res = await fetch(`/api/launch?worker=${id}`);
                const data = await res.json();
                alert(data.message);
                refreshData();
            } catch (err) {
                alert("Gagal meluncurkan worker: " + err);
            }
        }

        async function resetWorkerProfile(id) {
            if (confirm(`Yakin ingin me-reset profil Worker #${id}?\n\nSemua data login, cookies, dan session akun yang terkena restrict akan dihapus total sehingga profil kembali bersih seperti baru.`)) {
                try {
                    const res = await fetch(`/api/reset?worker=${id}`);
                    const data = await res.json();
                    alert(data.message);
                    refreshData();
                } catch (err) {
                    alert("Gagal reset profil: " + err);
                }
            }
        }

        async function launchAllChromes() {
            try {
                const res = await fetch('/api/launch-all');
                const data = await res.json();
                alert(data.message);
                setTimeout(refreshData, 1500);
            } catch (err) {
                alert("Gagal: " + err);
            }
        }

        function launchCustomBatch() {
            const w = document.getElementById('cfgWorkers').value;
            const t = document.getElementById('cfgTasks').value || 50;
            const mode = document.getElementById('cfgBrowserMode').value;

            // Kumpulkan daftar batch yang dicentang
            const checkedBatches = [];
            for (let i = 1; i <= 4; i++) {
                const el = document.getElementById('chkBatch' + i);
                if (el && el.checked) {
                    checkedBatches.push(i);
                }
            }

            if (checkedBatches.length === 0) {
                alert("Pilih minimal satu batch untuk dijalankan!");
                return;
            }

            const activeBatchesStr = checkedBatches.join(',');
            let cmd = `batch-follow --active-batches ${activeBatchesStr} --workers-per-batch ${w} --tasks ${t}`;
            if (mode === 'keep') {
                cmd += ' --keep-open';
            }

            startCommand(cmd);
        }

        async function startCommand(cmd) {
            try {
                const res = await fetch(`/api/start-cmd?cmd=${encodeURIComponent(cmd)}`);
                const data = await res.json();
                alert(data.message);
                setTimeout(refreshData, 1500);
            } catch (err) {
                alert("Gagal: " + err);
            }
        }

        async function stopEngine() {
            if (confirm("Hentikan proses bot engine sekarang?")) {
                try {
                    const res = await fetch('/api/stop');
                    const data = await res.json();
                    alert(data.message);
                    refreshData();
                } catch (err) {
                    alert("Gagal: " + err);
                }
            }
        }

        // Auto-refresh every 2.5s
        refreshData();
        setInterval(refreshData, 2500);
    </script>
</body>
</html>
'@

# Main Server Loop
try {
    while ($listener.IsListening) {
        $context = $null
        try {
            $context = $listener.GetContext()
        } catch {
            break
        }

        try {
            $request = $context.Request
            $response = $context.Response
            $response.Headers.Add("Access-Control-Allow-Origin", "*")

            $rawUrl = $request.RawUrl
            $buffer = $null

            if ($rawUrl -eq "/" -or $rawUrl -eq "/index.html") {
                $response.ContentType = "text/html; charset=utf-8"
                $buffer = [System.Text.Encoding]::UTF8.GetBytes($htmlPage)
            }
            elseif ($rawUrl -eq "/api/status") {
                $response.ContentType = "application/json; charset=utf-8"
                $jsonStr = Get-StatusJson
                $buffer = [System.Text.Encoding]::UTF8.GetBytes($jsonStr)
            }
            elseif ($rawUrl -like "/api/launch?worker=*") {
                $wId = [int]($rawUrl.Split("=")[1])
                $res = Launch-WorkerChrome -workerId $wId
                $response.ContentType = "application/json; charset=utf-8"
                $buffer = [System.Text.Encoding]::UTF8.GetBytes(($res | ConvertTo-Json))
            }
            elseif ($rawUrl -like "/api/reset?worker=*") {
                $wId = [int]($rawUrl.Split("=")[1])
                $res = Reset-WorkerProfile -workerId $wId
                $response.ContentType = "application/json; charset=utf-8"
                $buffer = [System.Text.Encoding]::UTF8.GetBytes(($res | ConvertTo-Json))
            }
            elseif ($rawUrl -eq "/api/launch-all") {
                Start-Process powershell -ArgumentList "-NoExit", "-File", (Join-Path $PSScriptRoot "start-8-chromes.ps1")
                $res = @{ success = $true; message = "Membuka 8 Chrome instances..." }
                $response.ContentType = "application/json; charset=utf-8"
                $buffer = [System.Text.Encoding]::UTF8.GetBytes(($res | ConvertTo-Json))
            }
            elseif ($rawUrl -like "/api/start-cmd?*") {
                $cmdParam = $rawUrl.Substring($rawUrl.IndexOf('=') + 1)
                $cmd = [System.Uri]::UnescapeDataString($cmdParam)
                $targetDir = $PSScriptRoot
                Start-Process powershell -ArgumentList "-NoExit", "-Command", "Set-Location '$targetDir'; cargo run --release -- $cmd"
                $res = @{ success = $true; message = "Bot dijalankan di jendela terminal baru: cargo run --release -- $cmd" }
                $response.ContentType = "application/json; charset=utf-8"
                $buffer = [System.Text.Encoding]::UTF8.GetBytes(($res | ConvertTo-Json))
            }
            elseif ($rawUrl -eq "/api/stop") {
                Get-Process -Name "rust-browser-click-engine" -ErrorAction SilentlyContinue | Stop-Process -Force
                $res = @{ success = $true; message = "Engine bot berhasil dihentikan." }
                $response.ContentType = "application/json; charset=utf-8"
                $buffer = [System.Text.Encoding]::UTF8.GetBytes(($res | ConvertTo-Json))
            }
            else {
                $response.StatusCode = 404
            }

            if ($buffer) {
                $response.ContentLength64 = $buffer.Length
                $response.OutputStream.Write($buffer, 0, $buffer.Length)
            }
        }
        catch [System.Net.HttpListenerException] {
            # Client browser disconnected or refreshed tab early; safe to ignore
        }
        catch {
            # Ignore transient request-level errors to keep server running
        }
        finally {
            if ($context -and $context.Response) {
                try { $context.Response.OutputStream.Close() } catch {}
            }
        }
    }
} finally {
    try { $listener.Stop() } catch {}
}
