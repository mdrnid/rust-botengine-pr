use crate::browser::Browser;
use crate::social_task::{LoopResult, SafetyBreakDetails, SocialAutomation};
use anyhow::{Context, Result};
use rand::Rng;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BatchState {
    pub batch_id: usize,
    pub ports: Vec<u16>,
    pub cooldown_until: Option<Instant>,
    pub total_tasks: usize,
    pub runs_completed: usize,
}

pub fn find_chrome_binary() -> Result<std::path::PathBuf> {
    let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    let program_files_x86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();

    let candidates = [
        format!(r"{program_files}\Google\Chrome\Application\chrome.exe"),
        format!(r"{program_files_x86}\Google\Chrome\Application\chrome.exe"),
        format!(r"{local_app_data}\Google\Chrome\Application\chrome.exe"),
        r"C:\Program Files\Google\Chrome\Application\chrome.exe".to_string(),
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe".to_string(),
    ];

    for c in &candidates {
        let p = std::path::Path::new(c);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }

    anyhow::bail!("chrome.exe tidak ditemukan di direktori Program Files.");
}

pub fn get_worker_profile(worker_id: usize) -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| std::env::var("HOME").unwrap_or_else(|_| ".".into()));
    if worker_id == 1 {
        std::path::PathBuf::from(home).join(".browser-automation-profile")
    } else {
        std::path::PathBuf::from(home).join(format!(".browser-automation-profile-{worker_id}"))
    }
}

pub fn launch_worker_chrome(
    chrome_path: &std::path::Path,
    worker_id: usize,
    port: u16,
    slot_idx: usize,
    total_workers_in_batch: usize,
) -> Result<()> {
    let profile = get_worker_profile(worker_id);

    // Dynamic grid layout: tiles up to 8 windows neatly on screen
    let (x, y, w, h) = if total_workers_in_batch <= 2 {
        if slot_idx % 2 == 0 {
            (0, 0, 765, 820)
        } else {
            (770, 0, 765, 820)
        }
    } else if total_workers_in_batch <= 4 {
        match slot_idx {
            0 => (0, 0, 765, 430),
            1 => (770, 0, 765, 430),
            2 => (0, 440, 765, 430),
            _ => (770, 440, 765, 430),
        }
    } else {
        // 5..8 workers: 4 columns x 2 rows grid
        let col = (slot_idx % 4) as i32;
        let row = ((slot_idx / 4) % 2) as i32;
        (col * 480, row * 440, 475, 430)
    };

    let args = [
        format!("--remote-debugging-port={port}"),
        "--remote-debugging-address=127.0.0.1".into(),
        format!("--user-data-dir={}", profile.display()),
        format!("--window-position={x},{y}"),
        format!("--window-size={w},{h}"),
        "--disable-blink-features=AutomationControlled".into(),
        "--disable-features=IsolateOrigins,site-per-process".into(),
        "--disable-infobars".into(),
        "--flag-switches-begin".into(),
        "--flag-switches-end".into(),
        "--disable-background-timer-throttling".into(),
        "--disable-backgrounding-occluded-windows".into(),
        "--disable-renderer-backgrounding".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--js-flags=--max-old-space-size=256".into(),
        "--disk-cache-size=10485760".into(),
        "--media-cache-size=10485760".into(),
        "--disable-extensions".into(),
        "--disable-component-update".into(),
        "--disable-sync".into(),
        "--disable-default-apps".into(),
        "--mute-audio".into(),
        "--renderer-process-limit=2".into(),
        "--disable-speech-api".into(),
        "--disable-breakpad".into(),
        "--disable-domain-reliability".into(),
        "https://www.pr.social/tasks".into(),
    ];

    std::process::Command::new(chrome_path)
        .args(&args)
        .spawn()
        .with_context(|| format!("Gagal meluncurkan Chrome Worker #{worker_id} pada port {port}"))?;

    Ok(())
}

pub async fn wait_for_endpoint_ready(endpoint: &str, timeout_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if let Ok((_base, targets)) = Browser::fetch_targets(endpoint).await {
            let has_page = targets.iter().any(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"));
            if has_page {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

pub async fn close_chrome_at_port(port: u16) {
    // 1. Tutup elegan via CDP Browser.close
    let client = reqwest::Client::builder().timeout(Duration::from_millis(1500)).build();
    if let Ok(client) = client {
        let version_url = format!("http://127.0.0.1:{port}/json/version");
        if let Ok(res) = client.get(&version_url).send().await {
            if let Ok(json) = res.json::<serde_json::Value>().await {
                if let Some(ws_url) = json.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
                    if let Ok(cdp) = crate::cdp::CdpClient::connect(ws_url).await {
                        let _ = cdp.command("Browser.close", serde_json::json!({})).await;
                        tokio::time::sleep(Duration::from_millis(800)).await;
                    }
                }
            }
        }
    }

    // 2. Fallback jika masih aktif di Windows
    #[cfg(windows)]
    {
        let check_url = format!("http://127.0.0.1:{port}/json/version");
        let still_alive = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .ok()
            .map(|c| c.get(&check_url).send());

        let is_running = if let Some(req) = still_alive {
            req.await.is_ok()
        } else {
            false
        };

        if is_running {
            let _ = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Get-Process chrome -ErrorAction SilentlyContinue | Where-Object {{ $_.CommandLine -like '*remote-debugging-port={port}*' }} | Stop-Process -Force -ErrorAction SilentlyContinue"
                    ),
                ])
                .output();
        }
    }
}

pub async fn run_batch_system(
    base_port: u16,
    num_batches: usize,
    active_batches: Option<String>,
    workers_per_batch: usize,
    tasks_per_worker: usize,
    max_cycles: Option<usize>,
    min_break_jitter: u64,
    max_break_jitter: u64,
    auto_manage_browser: bool,
) -> Result<()> {
    println!("================================================================================");
    println!("   PR.SOCIAL BATCH ROTATION ENGINE - ON-DEMAND AUTO-MANAGE (ASD-STE100)");
    println!("================================================================================");

    // Inisialisasi daftar batch yang aktif (bisa kustom seperti "2,3" atau default 1..N)
    let batch_ids: Vec<usize> = if let Some(ref ab) = active_batches {
        let parsed: Vec<usize> = ab
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .filter(|&id| id >= 1)
            .collect();
        if parsed.is_empty() {
            (1..=num_batches).collect()
        } else {
            parsed
        }
    } else {
        (1..=num_batches).collect()
    };

    println!("Konfigurasi Sistem:");
    println!("- Daftar Batch Aktif    : {:?}", batch_ids);
    println!("- Worker per Batch       : {}", workers_per_batch);
    println!("- Target Task per Worker : {} task", tasks_per_worker);
    println!("- Total per Batch        : {} task", workers_per_batch * tasks_per_worker);
    println!("- Manajemen Browser      : {}", if auto_manage_browser { "Auto Open-Close (Hemat RAM ~75%, hanya jendela aktif terbuka)" } else { "Tetap Standby (Semua browser terbuka)" });
    println!("- Smart Skip Cooldown    : AKTIF (Otomatis lompat ke batch yang siap jika batch saat ini masih timeout)");
    println!("- Jeda Tambahan Jitter   : {}-{} detik (1-2 menit)", min_break_jitter, max_break_jitter);
    println!("- Siklus Rotasi          : {}", max_cycles.map_or("Tanpa batas (Continuous Loop)".to_string(), |c| format!("{c} siklus")));
    println!("--------------------------------------------------------------------------------");

    let mut batches: Vec<BatchState> = batch_ids
        .iter()
        .map(|&b_idx| {
            let start_port = base_port + ((b_idx - 1) * workers_per_batch) as u16;
            let ports: Vec<u16> = (0..workers_per_batch).map(|w| start_port + w as u16).collect();
            BatchState {
                batch_id: b_idx,
                ports,
                cooldown_until: None,
                total_tasks: 0,
                runs_completed: 0,
            }
        })
        .collect();

    for b in &batches {
        println!("  [Batch #{}] Port Debugging: {:?}", b.batch_id, b.ports);
    }
    println!("================================================================================\n");

    let chrome_path = if auto_manage_browser {
        Some(find_chrome_binary()?)
    } else {
        None
    };

    let mut current_cycle = 1;

    loop {
        if let Some(max) = max_cycles {
            if current_cycle > max {
                println!("\nSemua siklus batch ({max}) telah selesai!");
                break;
            }
        }

        println!("\n################################################################################");
        println!("                         MEMULAI SIKLUS ROTASI KE-{}", current_cycle);
        println!("################################################################################");

        let mut active_batches_run = 0;

        for b_idx in 0..batches.len() {
            let bid = batches[b_idx].batch_id;
            let ports = batches[b_idx].ports.clone();

            println!("\n================================================================================");
            println!("  [BATCH #{}] Mengelola giliran Batch #{} pada port: {:?}", bid, bid, ports);
            println!("================================================================================");

            // 1. Periksa apakah batch ini masih dalam cooldown safety break dari siklus sebelumnya
            if let Some(deadline) = batches[b_idx].cooldown_until {
                let now = Instant::now();
                if now < deadline {
                    let remaining = (deadline - now).as_secs();

                    // SMART SKIP: Cek apakah ada batch LAIN yang SUDAH SIAP (tidak sedang cooldown)
                    let other_ready = batches.iter().enumerate().any(|(idx, other)| {
                        if idx == b_idx {
                            return false;
                        }
                        match other.cooldown_until {
                            None => true,
                            Some(d) => Instant::now() >= d,
                        }
                    });

                    if other_ready {
                        println!("\n⚡ [SMART SKIP] [Batch #{bid}] Masih dalam periode safety break (sisa {} detik / {:.1} menit).", remaining, remaining as f64 / 60.0);
                        println!("⚡ [SMART SKIP] Ada batch lain yang sudah SIAP. Melompati Batch #{bid} dan lanjut ke batch berikutnya!");
                        continue;
                    } else {
                        // Semua batch sedang dalam cooldown!
                        println!("\n⏳ [Batch #{bid}] Semua batch yang aktif sedang cooldown (Batch #{bid} tersisa {} detik / {:.1} menit).", remaining, remaining as f64 / 60.0);
                        println!("⏳ [Batch #{bid}] Browser ditutup saat istirahat untuk menghemat 100% RAM.");
                        println!("⏳ [Batch #{bid}] Menunggu dengan hitung mundur sebelum membuka Batch #{bid}...");
                        countdown_display(bid, remaining, 15).await;
                        println!("⏳ [Batch #{bid}] Hitung mundur selesai!");
                        batches[b_idx].cooldown_until = None;
                    }
                } else {
                    batches[b_idx].cooldown_until = None;
                }
            }

            // 2. Jika auto_manage_browser aktif, luncurkan Chrome untuk batch ini
            if let Some(ref c_path) = chrome_path {
                println!("🌐 [Batch #{bid}] Meluncurkan {} instance Chrome (On-Demand)...", ports.len());
                for (slot_idx, port) in ports.iter().enumerate() {
                    let worker_id = (bid - 1) * workers_per_batch + slot_idx + 1;
                    let ep = format!("http://127.0.0.1:{port}");
                    if Browser::fetch_targets(&ep).await.is_err() {
                        if let Err(e) = launch_worker_chrome(c_path, worker_id, *port, slot_idx, ports.len()) {
                            eprintln!("  [Batch #{bid}] Peringatan saat meluncurkan Worker #{worker_id}: {e:#}");
                        }
                    }
                }

                println!("⏳ [Batch #{bid}] Menunggu kesiapan browser CDP...");
                for port in &ports {
                    let ep = format!("http://127.0.0.1:{port}");
                    let _ = wait_for_endpoint_ready(&ep, 8).await;
                }
            }

            // 3. Cek endpoint aktif pada batch ini
            let mut online_endpoints: Vec<(usize, String)> = Vec::new();
            for (slot_idx, port) in ports.iter().enumerate() {
                let ep = format!("http://127.0.0.1:{port}");
                if let Ok((_base, targets)) = Browser::fetch_targets(&ep).await {
                    let has_page = targets.iter().any(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"));
                    if has_page {
                        let worker_id = (bid - 1) * workers_per_batch + slot_idx + 1;
                        online_endpoints.push((worker_id, ep));
                    }
                }
            }

            if online_endpoints.is_empty() {
                println!("  [Batch #{bid}] Port {:?} OFFLINE (Browser tidak dapat dihubungkan).", ports);
                println!("  [Batch #{bid}] Melompati ke batch berikutnya...\n");
                continue;
            }

            if online_endpoints.len() < ports.len() {
                println!("  [Batch #{bid}] Perhatian: Hanya {} dari {} worker online: {:?}", online_endpoints.len(), ports.len(), online_endpoints);
            }

            active_batches_run += 1;

            let total_online = online_endpoints.len();

            // 4. Jalankan worker pada batch ini secara paralel
            println!("🚀 [Batch #{bid}] Menjalankan {} worker untuk target {} task/worker...", total_online, tasks_per_worker);

            let mut handles = Vec::new();
            for (worker_id, endpoint) in online_endpoints {
                let tasks = tasks_per_worker;

                let handle = tokio::spawn(async move {
                    match SocialAutomation::connect(worker_id, &endpoint, 1, 2, 0, true).await {
                        Ok(mut runner) => runner.run_loop(Some(tasks), None).await,
                        Err(e) => {
                            eprintln!("[Worker #{worker_id}] Gagal terhubung ke {endpoint}: {e:#}");
                            Err(e)
                        }
                    }
                });

                handles.push(handle);
            }

            let results = futures_util::future::join_all(handles).await;

            // 5. Analisis hasil kerja batch ini
            let mut safety_break_encountered: Option<SafetyBreakDetails> = None;
            let mut account_not_found_count = 0;
            let mut checkpoint_count = 0;
            let mut batch_tasks_done = 0;

            for res in results {
                match res {
                    Ok(Ok(LoopResult::Completed(count))) => {
                        batch_tasks_done += count;
                    }
                    Ok(Ok(LoopResult::SafetyBreak(details))) => {
                        safety_break_encountered = Some(details);
                    }
                    Ok(Ok(LoopResult::AccountNotFound(msg))) => {
                        account_not_found_count += 1;
                        eprintln!("  [Batch #{bid}] Worker berhenti: {msg}");
                    }
                    Ok(Ok(LoopResult::InstagramCheckpoint(msg))) => {
                        checkpoint_count += 1;
                        eprintln!("  [Batch #{bid}] Worker terkena checkpoint: {msg}");
                    }
                    Ok(Err(e)) => {
                        let e_str = format!("{e:#}");
                        if e_str.contains("ACCOUNT_NOT_FOUND") {
                            account_not_found_count += 1;
                        } else if e_str.contains("INSTAGRAM_CHECKPOINT") {
                            checkpoint_count += 1;
                        }
                        eprintln!("  [Batch #{bid}] Error worker: {e:#}");
                    }
                    Err(e) => {
                        eprintln!("  [Batch #{bid}] Join error: {e:#}");
                    }
                }
            }

            batches[b_idx].total_tasks += batch_tasks_done;
            batches[b_idx].runs_completed += 1;

            if checkpoint_count > 0 && (checkpoint_count >= total_online || batch_tasks_done == 0) {
                println!("\n🛑 ================================================================================");
                println!("  [Batch #{bid}] TERDETEKSI INSTAGRAM CHECKPOINT / CHALLENGE VERIFIKASI!");
                println!("  Akun pada Batch #{bid} memerlukan verifikasi manual (SMS/Email/Captcha).");
                println!("  Batch #{bid} diistirahatkan 2 jam dan langsung berpindah ke batch berikutnya!");
                println!("================================================================================\n");
                batches[b_idx].cooldown_until = Some(Instant::now() + Duration::from_secs(7200));
            } else if (account_not_found_count >= total_online && total_online > 0) || (account_not_found_count > 0 && batch_tasks_done == 0) {
                println!("\n⚠️ ================================================================================");
                println!("  [Batch #{bid}] SEMUA WORKER BERHENTI: 'We cannot find your Instagram account'!");
                println!("  Akun Instagram pada Batch #{bid} tidak ditemukan atau bermasalah.");
                println!("  Batch #{bid} langsung dilewati untuk berpindah ke batch berikutnya!");
                println!("================================================================================\n");
                // Istirahatkan batch 60 menit agar putaran berikutnya otomatis di-Smart Skip
                batches[b_idx].cooldown_until = Some(Instant::now() + Duration::from_secs(3600));
            } else if let Some(details) = safety_break_encountered {
                let mut remaining_sec = details.remaining_seconds;
                if remaining_sec == 0 {
                    remaining_sec = 38 * 60; // Fallback 38 menit sesuai standar timeout PR.Social
                }

                let jitter = rand::rng().random_range(min_break_jitter..=max_break_jitter);
                let total_wait = remaining_sec + jitter;

                batches[b_idx].cooldown_until = Some(Instant::now() + Duration::from_secs(total_wait));

                println!("\n🛑 ================================================================================");
                println!("  [Batch #{bid}] PERINGATAN KESELAMATAN AKUN (SAFETY BREAK TERDETEKSI)");
                println!("  Pesan Website        : 'A break to keep your accounts safe'");
                println!("  Durasi Website       : {} (sisa {} detik)", details.display_duration, remaining_sec);
                println!("  Random Jitter        : {} detik (1-2 menit)", jitter);
                println!("  Total Waktu Tunggu   : {} detik ({:.1} menit)", total_wait, total_wait as f64 / 60.0);
                println!("  Batch #{bid} dijadwalkan istirahat dan akan dilanjutkan pada giliran berikutnya.");
                println!("================================================================================\n");
            } else {
                println!("\n✅ [Batch #{bid}] SELESAI! Task selesai pada batch ini: {batch_tasks_done}");
            }

            // 6. Tutup browser batch ini jika auto_manage_browser aktif
            if auto_manage_browser {
                println!("🔒 [Batch #{bid}] Menutup browser Chrome Batch #{bid} secara bersih (menghemat RAM)...");
                for port in &ports {
                    close_chrome_at_port(*port).await;
                }
                println!("🔒 [Batch #{bid}] Browser Batch #{bid} telah ditutup dengan aman.\n");
            }

            println!("  [Batch #{bid}] Berpindah ke batch berikutnya dalam 3 detik...\n");
            tokio::time::sleep(Duration::from_secs(3)).await;
        }

        if active_batches_run == 0 {
            println!("\n⚠️ Tidak ada worker yang dapat dijalankan pada semua batch.");
            println!("   Mengecek kembali dalam 10 detik...");
            tokio::time::sleep(Duration::from_secs(10)).await;
        } else {
            current_cycle += 1;
        }
    }

    Ok(())
}

async fn countdown_display(bid: usize, total_secs: u64, print_interval: u64) {
    for remaining in (1..=total_secs).rev() {
        if remaining % print_interval == 0 || remaining <= 5 {
            println!("  [Batch #{bid}] Sisa waktu istirahat: {} detik ({:.1} menit)...", remaining, remaining as f64 / 60.0);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
