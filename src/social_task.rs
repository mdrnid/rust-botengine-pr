    use crate::browser::Browser;
use crate::input;
use crate::logger::{TaskLogger, TaskRecord};
use anyhow::{Context, Result};
use chrono::Local;
use rand::Rng;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SafetyBreakDetails {
    pub detected: bool,
    #[serde(rename = "untilMs")]
    pub until_ms: Option<f64>,
    #[serde(rename = "displayDuration", default)]
    pub display_duration: String,
    #[serde(rename = "remainingSeconds", default)]
    pub remaining_seconds: u64,
}

#[derive(Debug, Clone)]
pub enum LoopResult {
    Completed(usize),
    SafetyBreak(SafetyBreakDetails),
    AccountNotFound(String),
    InstagramCheckpoint(String),
}

pub struct SocialAutomation {
    pub worker_id: usize,
    pr_browser: Browser,
    ig_browser: Browser,
    logger: TaskLogger,
    min_delay_secs: u64,
    max_delay_secs: u64,
    batch_min: usize,
    batch_max: usize,
    cooldown_min_secs: u64,
    cooldown_max_secs: u64,
    skip_chance_pct: u32,
    initial_delay_secs: u64,
    pub fast_mode: bool,
}

impl SocialAutomation {
    pub async fn connect(
        worker_id: usize,
        endpoint: &str,
        min_delay_secs: u64,
        max_delay_secs: u64,
        initial_delay_secs: u64,
        fast_mode: bool,
    ) -> Result<Self> {
        println!("[Worker #{worker_id}] Connecting to PR.Social tab at {endpoint}...");
        let pr_browser = Browser::connect(endpoint, Some("pr.social")).await
            .with_context(|| format!("[Worker #{worker_id}] Cannot find PR.Social tab on {endpoint}. Ensure PR.Social is open."))?;

        println!("[Worker #{worker_id}] Connecting or preparing Instagram tab at {endpoint}...");
        let ig_browser = Browser::ensure_tab(endpoint, "instagram.com", "https://www.instagram.com").await
            .with_context(|| format!("[Worker #{worker_id}] Cannot connect to Instagram tab on {endpoint}."))?;

        let logger = TaskLogger::new(worker_id)?;

        Ok(Self {
            worker_id,
            pr_browser,
            ig_browser,
            logger,
            min_delay_secs,
            max_delay_secs,
            batch_min: if fast_mode { usize::MAX } else { 12 },
            batch_max: if fast_mode { usize::MAX } else { 20 },
            cooldown_min_secs: if fast_mode { 0 } else { 90 },
            cooldown_max_secs: if fast_mode { 0 } else { 180 },
            skip_chance_pct: if fast_mode { 0 } else { 5 },
            initial_delay_secs: if fast_mode { 0 } else { initial_delay_secs },
            fast_mode,
        })
    }

    pub async fn run_loop(&mut self, max_tasks: Option<usize>, session_limit: Option<usize>) -> Result<LoopResult> {
        let wid = self.worker_id;
        let session_start = Instant::now();

        if !self.fast_mode && self.initial_delay_secs > 0 {
            println!("[Worker #{wid}] Desync delay: menunggu {} detik sebelum mulai...", self.initial_delay_secs);
            tokio::time::sleep(Duration::from_secs(self.initial_delay_secs)).await;
        }

        let mut completed: usize = 0;
        let mut errors: usize = 0;
        let mut session_completed: usize = 0;
        let mut next_batch_target = if self.fast_mode {
            usize::MAX
        } else {
            rand::rng().random_range(self.batch_min..=self.batch_max)
        };

        let session_cap = if self.fast_mode {
            None
        } else {
            session_limit.map(|base| {
                let jitter = rand::rng().random_range(0..=15);
                let cap = base.saturating_sub(8) + jitter;
                println!("[Worker #{wid}] Session limit: istirahat panjang setelah {cap} task.");
                cap
            })
        };

        if self.fast_mode {
            println!("[Worker #{wid}] >>> FAST MODE AKTIF <<<");
            println!("[Worker #{wid}] Mode Cepat: Jeda antar-task {}s, tanpa random jitter, tanpa scroll explore. Cooldown 1 menit setiap 10 task.", self.min_delay_secs);
        } else {
            println!("[Worker #{wid}] Jeda antar-task: {}-{} detik (random jitter)", self.min_delay_secs, self.max_delay_secs);
            println!("[Worker #{wid}] Batch cooldown setelah {} task pertama.", next_batch_target);
        }

        loop {
            if let Some(max) = max_tasks {
                if completed >= max {
                    println!("\n[Worker #{wid}] Reached maximum tasks ({max}). Done!");
                    break;
                }
            }

            // Session limit (only in normal mode)
            if !self.fast_mode {
                if let Some(cap) = session_cap {
                    if session_completed >= cap {
                        let break_secs = rand::rng().random_range(900..=1800);
                        println!("\n[Worker #{wid}] [SESSION BREAK] {session_completed} task selesai. Istirahat {:.0} menit...", break_secs as f64 / 60.0);
                        countdown(wid, break_secs, 60).await;
                        session_completed = 0;
                        let new_cap = session_limit.unwrap_or(80).saturating_sub(8) + rand::rng().random_range(0..=15_usize);
                        println!("[Worker #{wid}] Session berikutnya: istirahat setelah {new_cap} task.");
                        next_batch_target = completed + rand::rng().random_range(self.batch_min..=self.batch_max);
                    }
                }
            }

            // Random skip (only in normal mode)
            if !self.fast_mode && self.skip_chance_pct > 0 {
                if rand::rng().random_range(0..100_u32) < self.skip_chance_pct {
                    println!("\n[Worker #{wid}] Melewati task (random skip).");
                    let _ = self.click_next_task_js().await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            }

            println!("\n========================================");
            println!("  [Worker #{wid}] Task #{}", completed + 1);
            println!("========================================");

            let task_start = Instant::now();

            match self.execute_single_task().await {
                Ok(info) => {
                    completed += 1;
                    session_completed += 1;
                    let dur = task_start.elapsed().as_millis() as u64;
                    println!("[Worker #{wid}] [OK] Task #{completed} selesai ({dur}ms): {info}");

                    if let Err(e) = self.logger.log_task(&TaskRecord {
                        timestamp: Local::now().to_rfc3339(),
                        worker_id: wid,
                        task_number: completed,
                        instagram_url: info.clone(),
                        action: "followed".into(),
                        duration_ms: dur,
                        status: "success".into(),
                        error: None,
                    }) {
                        eprintln!("[Worker #{wid}] Log error: {e}");
                    }

                    // Cooldown check
                    if self.fast_mode {
                        if completed > 0 && completed % 10 == 0 {
                            println!("\n[Worker #{wid}] [COOLDOWN] Berhasil menyelesaikan {} task. Istirahat 1 menit (60 detik)...", completed);
                            countdown(wid, 60, 10).await;
                            println!("[Worker #{wid}] Istirahat 1 menit selesai! Me-reload halaman PR.Social...");
                            let _ = self.pr_browser.bring_to_front().await;
                            let _ = self.pr_browser.reload().await;
                            tokio::time::sleep(Duration::from_millis(2000)).await;

                            let break_info = self.check_safety_break_details().await.unwrap_or_default();
                            if break_info.detected {
                                println!("\n🛑 [Worker #{wid}] TERDETEKSI: 'A break to keep your accounts safe'!");
                                if !break_info.display_duration.is_empty() {
                                    println!("   Durasi timeout website: {} (sisa {} detik)", break_info.display_duration, break_info.remaining_seconds);
                                }
                                println!("🛑 [Worker #{wid}] PR.Social mewajibkan istirahat akun. Worker #{wid} BERHENTI otomatis.\n");
                                let total_secs = session_start.elapsed().as_secs();
                                let _ = self.logger.write_summary(completed, errors, total_secs);
                                return Ok(LoopResult::SafetyBreak(break_info));
                            }
                            println!("[Worker #{wid}] Halaman PR.Social siap. Melanjutkan task berikutnya...\n");
                        }
                    } else if completed >= next_batch_target {
                        let cool = rand::rng().random_range(self.cooldown_min_secs..=self.cooldown_max_secs);
                        println!("\n[Worker #{wid}] [BATCH COOLDOWN] {completed} task. Istirahat {cool} detik...");
                        countdown(wid, cool, 15).await;
                        let next_step = rand::rng().random_range(self.batch_min..=self.batch_max);
                        next_batch_target = completed + next_step;
                        println!("[Worker #{wid}] Batch berikutnya setelah +{next_step} task (task ke-{next_batch_target}).\n");
                    }
                }
                Err(e) => {
                    errors += 1;
                    let dur = task_start.elapsed().as_millis() as u64;
                    let err_msg = format!("{e:#}");

                    if err_msg.contains("SAFETY_BREAK_DETECTED") {
                        let details = self.check_safety_break_details().await.unwrap_or_default();
                        println!("\n🛑 [Worker #{wid}] TERDETEKSI: 'A break to keep your accounts safe'!");
                        if !details.display_duration.is_empty() {
                            println!("   Durasi timeout website: {} (sisa {} detik)", details.display_duration, details.remaining_seconds);
                        }
                        println!("🛑 [Worker #{wid}] Worker #{wid} BERHENTI untuk menunggu cooldown.\n");
                        let total_secs = session_start.elapsed().as_secs();
                        let _ = self.logger.write_summary(completed, errors, total_secs);
                        return Ok(LoopResult::SafetyBreak(details));
                    }

                    let is_checkpoint = err_msg.contains("INSTAGRAM_CHECKPOINT")
                        || self.check_instagram_checkpoint().await.unwrap_or(false);

                    if is_checkpoint {
                        println!("\n🛑 [Worker #{wid}] FATAL TERDETEKSI: Instagram Checkpoint / Challenge Verifikasi!");
                        println!("🛑 [Worker #{wid}] Akun Instagram memerlukan verifikasi manual (SMS/Email/Captcha).");
                        println!("🛑 [Worker #{wid}] Menghentikan Worker #{wid} agar akun aman dari penangguhan.\n");
                        let total_secs = session_start.elapsed().as_secs();
                        let _ = self.logger.write_summary(completed, errors, total_secs);
                        return Ok(LoopResult::InstagramCheckpoint("Instagram Checkpoint Verification".into()));
                    }

                    // Deteksi ACCOUNT_NOT_FOUND dari pesan error atau langsung dari inspect halaman PR.Social
                    let is_account_missing = err_msg.contains("ACCOUNT_NOT_FOUND")
                        || self.check_account_not_found().await.unwrap_or(false);

                    if is_account_missing {
                        println!("\n❌ [Worker #{wid}] FATAL TERDETEKSI: 'We cannot find your Instagram account'!");
                        println!("❌ [Worker #{wid}] Akun Instagram tidak ditemukan / bermasalah pada profil ini.");
                        println!("❌ [Worker #{wid}] Menghentikan Worker #{wid} secara permanen.\n");
                        let total_secs = session_start.elapsed().as_secs();
                        let _ = self.logger.write_summary(completed, errors, total_secs);
                        return Ok(LoopResult::AccountNotFound("We cannot find your Instagram account".into()));
                    }

                    eprintln!("[Worker #{wid}] [ERROR] Task gagal: {e:#}");

                    let _ = self.logger.log_task(&TaskRecord {
                        timestamp: Local::now().to_rfc3339(),
                        worker_id: wid,
                        task_number: completed + 1,
                        instagram_url: String::new(),
                        action: "error".into(),
                        duration_ms: dur,
                        status: "error".into(),
                        error: Some(format!("{e:#}")),
                    });

                    // Auto-recovery / un-stuck: Reload PR.Social halaman tasks
                    println!("[Worker #{wid}] 🔄 Terjadi kendala / ngestuck ({e:#}). Me-reload halaman PR.Social ke https://www.pr.social/tasks...");
                    let _ = self.pr_browser.bring_to_front().await;
                    let _ = self.pr_browser.navigate("https://www.pr.social/tasks").await;

                    let err_wait = if self.fast_mode { 2 } else { 5 };
                    println!("[Worker #{wid}] Menunggu {err_wait} detik sebelum retry...");
                    tokio::time::sleep(Duration::from_secs(err_wait)).await;
                }
            }

            // Inter-task delay
            let task_delay = if self.fast_mode {
                self.min_delay_secs
            } else {
                weighted_delay(self.min_delay_secs, self.max_delay_secs)
            };

            if task_delay > 0 {
                println!("[Worker #{wid}] Jeda {task_delay} detik...");
                tokio::time::sleep(Duration::from_secs(task_delay)).await;
            }
        }

        let total_secs = session_start.elapsed().as_secs();
        let _ = self.logger.write_summary(completed, errors, total_secs);
        println!("[Worker #{wid}] Selesai. Total: {completed} task, {errors} error, {total_secs}s. Log: {}", self.logger.path().display());

        Ok(LoopResult::Completed(completed))
    }

    pub async fn execute_single_task(&self) -> Result<String> {
        let wid = self.worker_id;

        // 1. Focus PR.Social tab lebih dulu agar context halaman aktif
        self.pr_browser.bring_to_front().await?;
        let switch_wait = if self.fast_mode { 100 } else { rand::rng().random_range(500..=1200_u64) };
        tokio::time::sleep(Duration::from_millis(switch_wait)).await;

        // 0. Check for account safety break warning
        if self.check_safety_break().await.unwrap_or(false) {
            anyhow::bail!("SAFETY_BREAK_DETECTED: A break to keep your accounts safe");
        }

        // Check for "We cannot find your Instagram account"
        if self.check_account_not_found().await.unwrap_or(false) {
            anyhow::bail!("ACCOUNT_NOT_FOUND: We cannot find your Instagram account");
        }

        // 2. Read time simulation (skipped in fast mode)
        if !self.fast_mode {
            let read_wait = rand::rng().random_range(1200..=3000_u64);
            tokio::time::sleep(Duration::from_millis(read_wait)).await;
        }

        // 3. Start task and extract Instagram link
        let target_url = self.start_task_and_get_link().await?;
        println!("[Worker #{wid}] [PR.Social] Target: {target_url}");

        // 4. Switch to Instagram tab
        println!("[Worker #{wid}] [Instagram] Navigasi ke profil...");
        self.ig_browser.bring_to_front().await?;
        let tab_switch_wait = if self.fast_mode { 100 } else { rand::rng().random_range(600..=1500_u64) };
        tokio::time::sleep(Duration::from_millis(tab_switch_wait)).await;

        self.ig_browser.navigate(&target_url).await?;

        // 5. Wait for page load
        let load_wait = if self.fast_mode { 1200 } else { rand::rng().random_range(2500..=4000_u64) };
        tokio::time::sleep(Duration::from_millis(load_wait)).await;

        // Cek apakah akun terkena Checkpoint verifikasi Instagram
        if self.check_instagram_checkpoint().await.unwrap_or(false) {
            anyhow::bail!("INSTAGRAM_CHECKPOINT: Terdeteksi Checkpoint / Challenge verifikasi Instagram");
        }

        // Micro-scroll instan (trigger event scroll & interaksi nyata di viewport)
        let scroll_amount = rand::rng().random_range(120.0..=220.0);
        let _ = input::fast_micro_scroll(&self.ig_browser, scroll_amount).await;
        let scroll_wait = if self.fast_mode { 200 } else { 500 };
        tokio::time::sleep(Duration::from_millis(scroll_wait)).await;

        // 6. Browse profile (skipped in fast mode)
        if !self.fast_mode {
            self.browse_instagram_profile().await;
        }

        // 7. Click Follow button
        self.click_instagram_follow().await?;

        // Jeda 1.5 - 2.5 detik (random jitter) setelah follow agar status follow tercatat di server Instagram
        let check_jitter_ms = rand::rng().random_range(1500..=2500_u64);
        println!("[Worker #{wid}] Jeda {:.2} detik (jitter 1.5 - 2.5s) sebelum verifikasi (check my follow)...", check_jitter_ms as f64 / 1000.0);
        tokio::time::sleep(Duration::from_millis(check_jitter_ms)).await;

        // 8. Return to PR.Social
        println!("[Worker #{wid}] [PR.Social] Kembali ke PR.Social...");
        self.pr_browser.bring_to_front().await?;
        let return_wait = if self.fast_mode { 400 } else { rand::rng().random_range(700..=1500_u64) };
        tokio::time::sleep(Duration::from_millis(return_wait)).await;

        // 9. Check my follow (otomatis menangani 'A little too fast' & 'Check again' dengan jeda 1.0-1.5s)
        self.click_check_my_follow().await?;

        // 10. Next task
        self.click_next_task().await?;

        Ok(target_url)
    }

    async fn start_task_and_get_link(&self) -> Result<String> {
        let wid = self.worker_id;
        let script = r#"
        (() => {
            const rawHtml = (document.body ? document.body.innerHTML : '').toLowerCase();
            const rawText = (document.body ? (document.body.innerText || document.body.textContent || '') : '').toLowerCase();
            const cleanText = rawText.replace(/\s+/g, ' ');

            // Check for "A break to keep your accounts safe"
            if (rawHtml.includes('a break to keep your account') || cleanText.includes('a break to keep your account')) {
                return { status: "account_safety_break" };
            }

            // Check for "We cannot find your Instagram account"
            if (rawHtml.includes('cannot find your instagram') ||
                rawHtml.includes('cannot find your account') ||
                cleanText.includes('cannot find your instagram') ||
                cleanText.includes('cannot find your account') ||
                /(?:we )?can(?:not|\'t| not) find (?:your )?instagram/i.test(cleanText) ||
                /cannot find (?:your )?instagram/i.test(rawHtml)) {
                return { status: "account_not_found" };
            }

            const boldTags = Array.from(document.querySelectorAll('b, strong, .alert, .note, .warn, .popup, .modal'));
            if (boldTags.some(el => {
                const t = (el.innerText || el.textContent || '').toLowerCase().replace(/\s+/g, ' ');
                return t.includes('cannot find your instagram') || t.includes('cannot find your account');
            })) {
                return { status: "account_not_found" };
            }

            // Check for "No tasks on Instagram right now"
            const hasNoTasks = rawHtml.includes('no tasks on instagram right now')
                || rawHtml.includes('no tasks right now')
                || cleanText.includes('no tasks on instagram right now')
                || cleanText.includes('no tasks right now')
                || cleanText.includes('no tasks available');
            if (hasNoTasks) {
                return { status: "no_tasks_right_now" };
            }

            // Check for "You have another task in progress"
            const hasTaskInProgress = cleanText.includes('you have another task in progress')
                || cleanText.includes('finish “follow on instagram”')
                || cleanText.includes('finish "follow on instagram"')
                || cleanText.includes('then you can start this one');

            if (hasTaskInProgress) {
                const btns = Array.from(document.querySelectorAll('button, a, .btn, .btn-p, [role="button"], span.btn'));
                const openTaskBtn = btns.find(b => {
                    const t = (b.innerText || b.textContent || '').toLowerCase();
                    return t.includes('open that task') || t.includes('open task') || t.includes('open this task');
                });

                if (openTaskBtn && openTaskBtn.offsetParent !== null && !openTaskBtn.disabled) {
                    openTaskBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    openTaskBtn.click();
                    return { status: "clicked_open_that_task" };
                }
            }

            // Check for "A little too fast"
            const hasTooFast = rawHtml.includes('a little too fast') || cleanText.includes('a little too fast');
            if (hasTooFast) {
                return { status: "detected_too_fast" };
            }

            // Check "All places are taken" warning
            const noteWarn = document.querySelector('.note.warn, .note, div.warn');
            const hasTakenWarning = (noteWarn && (noteWarn.innerText || '').toLowerCase().includes('all places are taken'))
                || rawHtml.includes('all places are taken')
                || rawHtml.includes('someone took the last place')
                || cleanText.includes('all places are taken')
                || cleanText.includes('someone took the last place');

            if (hasTakenWarning) {
                let nextBtn = document.querySelector('button[data-go="next"], [data-go="next"]');
                if (!nextBtn) {
                    const btns = Array.from(document.querySelectorAll('button, .btn-p'));
                    nextBtn = btns.find(b => (b.innerText || '').toLowerCase().includes('next'));
                }
                if (nextBtn && nextBtn.offsetParent !== null && !nextBtn.disabled) {
                    nextBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    nextBtn.click();
                    return { status: "places_taken_clicked_next" };
                }
                return { status: "places_taken_waiting_next" };
            }

            // Check for Instagram copy link
            const copyBtn = document.querySelector('button[data-copy*="instagram.com"], [data-copy*="instagram.com"], button.qb[data-copy], [data-copy]');
            if (copyBtn) {
                const link = copyBtn.getAttribute('data-copy');
                copyBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                copyBtn.click();
                if (link && link.trim() && link.includes('instagram.com')) {
                    return { status: "found_link", link: link.trim() };
                }
            }

            const aTag = document.querySelector('a[href*="instagram.com"]');
            if (aTag && aTag.href) {
                return { status: "found_link", link: aTag.href };
            }

            // Click Start button
            const startBtn = document.querySelector('button[data-go="start"], [data-go="start"]');
            if (startBtn && startBtn.offsetParent !== null && !startBtn.disabled) {
                startBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                startBtn.click();
                return { status: "clicked_start", text: startBtn.innerText.trim() };
            }

            // Click available task row
            const rows = Array.from(document.querySelectorAll('button.trow, [data-coach="row"], [data-row]'));
            const availableRow = rows.find(r => {
                const t = (r.innerText || '').toLowerCase();
                return t.includes('instagram') && !t.includes('paid') && !t.includes('on review');
            }) || rows.find(r => {
                const t = (r.innerText || '').toLowerCase();
                return !t.includes('paid') && !t.includes('on review');
            });

            if (availableRow && availableRow.offsetParent !== null) {
                availableRow.scrollIntoView({ behavior: 'smooth', block: 'center' });
                availableRow.click();
                return { status: "clicked_row", text: availableRow.innerText.trim() };
            }

            return { status: "waiting" };
        })()
        "#;

        for _attempt in 1..=40 {
            let res = self.pr_browser.evaluate(script).await?;
            let status = res.get("status").and_then(Value::as_str).unwrap_or("waiting");

            match status {
                "found_link" => {
                    if let Some(link) = res.get("link").and_then(Value::as_str) {
                        return Ok(link.to_string());
                    }
                }
                "account_safety_break" => {
                    anyhow::bail!("SAFETY_BREAK_DETECTED: A break to keep your accounts safe");
                }
                "account_not_found" => {
                    anyhow::bail!("ACCOUNT_NOT_FOUND: We cannot find your Instagram account");
                }
                "no_tasks_right_now" => {
                    println!("[Worker #{wid}] ⚡ [PR.Social] Tidak ada task ('No tasks on Instagram right now'). Langsung reload instan!");
                    let _ = self.pr_browser.navigate("https://www.pr.social/tasks").await;
                    tokio::time::sleep(Duration::from_millis(if self.fast_mode { 250 } else { 800 })).await;
                    continue;
                }
                "clicked_open_that_task" => {
                    println!("[Worker #{wid}] ⚠️ [PR.Social] Terdeteksi 'You have another task in progress'!");
                    println!("[Worker #{wid}] 🔘 Menekan tombol 'Open that task'...");
                    let wait = if self.fast_mode { 350 } else { 1000 };
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                    continue;
                }
                "detected_too_fast" => {
                    let pre_delay = rand::rng().random_range(1000..=1500_u64);
                    println!("[Worker #{wid}] ⚠️ [PR.Social] Terdeteksi 'A little too fast'!");
                    println!("[Worker #{wid}] Menunggu jeda {:.2}s (1.0-1.5s) sebelum menekan tombol...", pre_delay as f64 / 1000.0);
                    tokio::time::sleep(Duration::from_millis(pre_delay)).await;
                    self.dismiss_too_fast_if_present().await;
                    continue;
                }
                "places_taken_clicked_next" => {
                    println!("[Worker #{wid}] [PR.Social] 'All places taken' — skipping to next...");
                    tokio::time::sleep(Duration::from_millis(if self.fast_mode { 300 } else { 1200 })).await;
                }
                "places_taken_waiting_next" => {
                    println!("[Worker #{wid}] [PR.Social] 'All places taken', menunggu Next...");
                    tokio::time::sleep(Duration::from_millis(if self.fast_mode { 200 } else { 500 })).await;
                }
                "clicked_start" => {
                    println!("[Worker #{wid}] [PR.Social] Start diklik...");
                    let wait = if self.fast_mode { 350 } else { rand::rng().random_range(1000..=2000_u64) };
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
                "clicked_row" => {
                    println!("[Worker #{wid}] [PR.Social] Task row diklik...");
                    let wait = if self.fast_mode { 250 } else { rand::rng().random_range(800..=1500_u64) };
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
                _ => {
                    if self.check_account_not_found().await.unwrap_or(false) {
                        anyhow::bail!("ACCOUNT_NOT_FOUND: We cannot find your Instagram account");
                    }
                    if _attempt == 14 || _attempt == 28 {
                        println!("[Worker #{wid}] ⚡ [PR.Social] Belum menemukan task setelah attempt ke-{_attempt}. Langsung reload instan...");
                        let _ = self.pr_browser.navigate("https://www.pr.social/tasks").await;
                        tokio::time::sleep(Duration::from_millis(if self.fast_mode { 300 } else { 1000 })).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(if self.fast_mode { 120 } else { 400 })).await;
                    }
                }
            }
        }

        anyhow::bail!("[Worker #{wid}] Timed out mencari task di PR.Social")
    }

    /// Browse the Instagram profile like a human: scroll around, look at posts.
    async fn browse_instagram_profile(&self) {
        let wid = self.worker_id;

        let scroll_amount = rand::rng().random_range(200.0..=550.0_f64);
        let browse_time = rand::rng().random_range(3000..=7000_u64);
        let scroll_back_ratio = rand::rng().random_range(0.6..=0.9_f64);

        println!("[Worker #{wid}] [Instagram] Melihat-lihat profil ({:.1}s)...", browse_time as f64 / 1000.0);

        if let Err(e) = input::human_scroll(&self.ig_browser, scroll_amount).await {
            eprintln!("[Worker #{wid}] Scroll down warning: {e}");
        }

        let midpoint_wait = rand::rng().random_range((browse_time / 2)..=browse_time);
        tokio::time::sleep(Duration::from_millis(midpoint_wait)).await;

        if let Err(e) = input::human_scroll(&self.ig_browser, -(scroll_amount * scroll_back_ratio)).await {
            eprintln!("[Worker #{wid}] Scroll up warning: {e}");
        }

        let remaining = browse_time.saturating_sub(midpoint_wait);
        if remaining > 300 {
            tokio::time::sleep(Duration::from_millis(remaining)).await;
        }
    }

    async fn click_instagram_follow(&self) -> Result<()> {
        let wid = self.worker_id;

        let find_and_click_script = r#"
        (() => {
            const candidates = Array.from(document.querySelectorAll('button, [role="button"], div[role="button"], a[role="button"]'));

            // 1. Cek apakah sudah di-follow
            const alreadyFollowed = candidates.find(el => {
                const text = (el.innerText || el.textContent || '').trim().toLowerCase();
                return text === 'following' || text === 'requested' || 
                       text === 'mengikuti' || text === 'diminta' ||
                       text.includes('following') || text.includes('mengikuti');
            });
            if (alreadyFollowed) {
                return { action: "already_followed", text: alreadyFollowed.innerText.trim() };
            }

            // 2. Cari tombol Follow dengan selector teks fleksibel (Inggris & Indonesia)
            const followBtn = candidates.find(el => {
                const text = (el.innerText || el.textContent || '').trim().toLowerCase();
                return text === 'follow' || text === 'follow back' || 
                       text === 'ikuti' || text === 'ikuti balik' ||
                       text === 'follow back?' || text === 'ikuti kembali';
            }) || candidates.find(el => {
                const text = (el.innerText || el.textContent || '').trim().toLowerCase();
                const isExclude = text.includes('message') || text.includes('pesan') || 
                                  text.includes('share') || text.includes('bagikan') ||
                                  text.includes('contact') || text.includes('kontak') ||
                                  text.includes('edit');
                return !isExclude && (text.startsWith('follow') || text.startsWith('ikuti'));
            });

            if (followBtn) {
                // Gunakan 'instant' agar koordinat rect langsung tepat tanpa delay animasi smooth
                followBtn.scrollIntoView({ behavior: 'instant', block: 'center' });
                const rect = followBtn.getBoundingClientRect();
                return {
                    action: "found",
                    text: followBtn.innerText.trim(),
                    rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height }
                };
            }

            return { action: "not_found" };
        })()
        "#;

        println!("[Worker #{wid}] [Instagram] Mencari tombol Follow...");
        let max_attempts = if self.fast_mode { 12 } else { 18 };

        for _attempt in 1..=max_attempts {
            let res = self.ig_browser.evaluate(find_and_click_script).await?;
            let action = res.get("action").and_then(Value::as_str).unwrap_or("not_found");

            match action {
                "already_followed" => {
                    let text = res.get("text").and_then(Value::as_str).unwrap_or("Following");
                    println!("[Worker #{wid}] [Instagram] Akun sudah di-follow ({text}). Skip.");
                    return Ok(());
                }
                "found" => {
                    let btn_text = res.get("text").and_then(Value::as_str).unwrap_or("Follow");
                    if let Some(rect) = res.get("rect") {
                        let rx = rect["x"].as_f64().unwrap_or(0.0);
                        let ry = rect["y"].as_f64().unwrap_or(0.0);
                        let rw = rect["width"].as_f64().unwrap_or(80.0);
                        let rh = rect["height"].as_f64().unwrap_or(30.0);

                        println!("[Worker #{wid}] [Instagram] Tombol '{btn_text}' ditemukan pada koordinat ({:.0}, {:.0}). Mengklik via Hardware CDP...", rx, ry);
                        input::fast_cdp_click(&self.ig_browser, rx, ry, rw, rh).await?;
                        println!("[Worker #{wid}] [Instagram] Tombol '{btn_text}' berhasil diklik!");
                    } else {
                        // Fallback JS click langsung jika rect tidak terbaca
                        self.ig_browser.evaluate(r#"
                            (() => {
                                const btns = Array.from(document.querySelectorAll('button, [role="button"]'));
                                const b = btns.find(el => {
                                    const t = (el.innerText || el.textContent || '').trim().toLowerCase();
                                    return t === 'follow' || t === 'follow back' || t === 'ikuti' || t === 'ikuti balik';
                                });
                                if (b) b.click();
                            })()
                        "#).await?;
                        println!("[Worker #{wid}] [Instagram] Tombol '{btn_text}' diklik (fallback JS)!");
                    }

                    // Verifikasi apakah tombol berubah status (Following / Requested / Mengikuti)
                    tokio::time::sleep(Duration::from_millis(if self.fast_mode { 600 } else { 1200 })).await;
                    let verify_script = r#"
                    (() => {
                        const btns = Array.from(document.querySelectorAll('button, [role="button"], div[role="button"]'));
                        return btns.some(el => {
                            const t = (el.innerText || el.textContent || '').trim().toLowerCase();
                            return t === 'following' || t === 'requested' || t === 'mengikuti' || t === 'diminta';
                        });
                    })()
                    "#;
                    if let Ok(verified) = self.ig_browser.evaluate(verify_script).await {
                        if verified.as_bool() == Some(true) {
                            println!("[Worker #{wid}] ✅ [Instagram] Terverifikasi: Status akun berubah menjadi 'Following'!");
                            return Ok(());
                        } else {
                            // Coba cadangan klik langsung jika status belum berubah
                            let _ = self.ig_browser.evaluate(r#"
                                (() => {
                                    const btns = Array.from(document.querySelectorAll('button, [role="button"]'));
                                    const b = btns.find(el => {
                                        const t = (el.innerText || el.textContent || '').trim().toLowerCase();
                                        return t === 'follow' || t === 'follow back' || t === 'ikuti' || t === 'ikuti balik';
                                    });
                                    if (b) b.click();
                                })()
                            "#).await;
                            println!("[Worker #{wid}] 🔄 [Instagram] Mencoba klik cadangan untuk memastikan tombol tertekan...");
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            return Ok(());
                        }
                    }

                    return Ok(());
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(if self.fast_mode { 250 } else { 500 })).await;
                }
            }
        }

        eprintln!("[Worker #{wid}] ⚠️ [Instagram] Tombol Follow tidak ditemukan setelah {} percobaan.", max_attempts);
        Ok(())
    }

    pub async fn dismiss_too_fast_if_present(&self) -> bool {
        let wid = self.worker_id;

        // 1. Cek apakah ada peringatan "A little too fast" di DOM tanpa langsung klik
        let check_script = r#"
        (() => {
            const allEls = Array.from(document.querySelectorAll('h3, h2, h1, b, strong, p, div, span, .note, .warn, .alert'));
            const match = allEls.find(el => {
                const t = (el.innerText || el.textContent || '').toLowerCase();
                return t.includes('a little too fast');
            });
            return { detected: !!match };
        })()
        "#;

        if let Ok(res) = self.pr_browser.evaluate(check_script).await {
            if res.get("detected").and_then(Value::as_bool) == Some(true) {
                let pre_delay = rand::rng().random_range(1000..=1500_u64);
                println!("[Worker #{wid}] ⚠️ [PR.Social] Terdeteksi peringatan 'A little too fast'!");
                println!("[Worker #{wid}] Menunggu jeda {:.2}s (1.0-1.5s) sebelum menekan 'Check again'...", pre_delay as f64 / 1000.0);
                tokio::time::sleep(Duration::from_millis(pre_delay)).await;

                // 2. Klik tombol "Check again" (<button class="btn-p" data-go="apicheck">) setelah jeda 1.0-1.5 detik
                let click_script = r#"
                (() => {
                    // Prioritas 1: Cari tombol spesifik PR.Social <button class="btn-p" data-go="apicheck">Check again</button>
                    let targetBtn = document.querySelector('button[data-go="apicheck"], [data-go="apicheck"]');
                    if (!targetBtn) {
                        const btns = Array.from(document.querySelectorAll('button, .btn-p, [role="button"]'));
                        targetBtn = btns.find(b => {
                            const t = (b.innerText || b.textContent || '').toLowerCase();
                            return t.includes('check again') || t.includes('check');
                        });
                    }

                    // Prioritas 2: Tombol / link di sekitar peringatan "a little too fast"
                    if (!targetBtn) {
                        const allEls = Array.from(document.querySelectorAll('b, strong, div, span, p'));
                        const match = allEls.find(el => (el.innerText || el.textContent || '').toLowerCase().includes('a little too fast'));
                        if (match) {
                            targetBtn = match.closest('button, a, [role="button"], .btn') || match;
                        }
                    }

                    if (targetBtn && targetBtn.offsetParent !== null && !targetBtn.disabled) {
                        targetBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                        targetBtn.click();
                        return { clicked: true, text: (targetBtn.innerText || targetBtn.textContent || '').trim() };
                    }
                    return { clicked: false };
                })()
                "#;

                if let Ok(click_res) = self.pr_browser.evaluate(click_script).await {
                    if click_res.get("clicked").and_then(Value::as_bool) == Some(true) {
                        let text = click_res.get("text").and_then(Value::as_str).unwrap_or("Check again");
                        println!("[Worker #{wid}] 🔘 Menekan tombol '{text}' (<button class=\"btn-p\" data-go=\"apicheck\">)!");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        return true;
                    }
                }
            }
        }
        false
    }

    async fn click_check_my_follow(&self) -> Result<()> {
        let wid = self.worker_id;
        let script = r#"
        (() => {
            // Jika tombol Next task sudah muncul, berarti follow sudah terverifikasi!
            let nextBtn = document.querySelector('button[data-go="next"], [data-go="next"]');
            if (nextBtn && nextBtn.offsetParent !== null && !nextBtn.disabled) {
                return { clicked: true, text: "Next task (verified)" };
            }

            let checkBtn = document.querySelector('button[data-go="apicheck"], [data-go="apicheck"]');
            if (!checkBtn) {
                const btns = Array.from(document.querySelectorAll('button, .btn-p'));
                checkBtn = btns.find(b => (b.innerText || '').toLowerCase().includes('check'));
            }
            if (checkBtn) {
                checkBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                if (!checkBtn.disabled) {
                    checkBtn.click();
                    return { clicked: true, text: checkBtn.innerText.trim() };
                }
                return { clicked: false, disabled: true };
            }
            return { clicked: false, not_found: true };
        })()
        "#;

        println!("[Worker #{wid}] [PR.Social] Klik 'Check my follow'...");
        let max_attempts = if self.fast_mode { 8 } else { 12 };
        for attempt in 1..=max_attempts {
            if self.check_account_not_found().await.unwrap_or(false) {
                anyhow::bail!("ACCOUNT_NOT_FOUND: We cannot find your Instagram account");
            }
            if self.dismiss_too_fast_if_present().await {
                // Tombol "Check again" sudah diklik dengan jeda 1.0-1.5s
                let verify_wait = if self.fast_mode { 800 } else { rand::rng().random_range(2500..=4000_u64) };
                tokio::time::sleep(Duration::from_millis(verify_wait)).await;
                return Ok(());
            }

            let res = self.pr_browser.evaluate(script).await?;
            if res.get("clicked").and_then(Value::as_bool) == Some(true) {
                let btn_text = res.get("text").and_then(Value::as_str).unwrap_or("Check");
                println!("[Worker #{wid}] [PR.Social] Tombol '{btn_text}' diklik.");
                let verify_wait = if self.fast_mode { 800 } else { rand::rng().random_range(2500..=4000_u64) };
                tokio::time::sleep(Duration::from_millis(verify_wait)).await;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(if self.fast_mode { 200 } else { 500 * attempt })).await;
        }

        anyhow::bail!("[Worker #{wid}] Gagal klik 'Check my follow'")
    }

    async fn click_next_task(&self) -> Result<()> {
        let wid = self.worker_id;
        let script = r#"
        (() => {
            let nextBtn = document.querySelector('button[data-go="next"], [data-go="next"]');
            if (!nextBtn) {
                const btns = Array.from(document.querySelectorAll('button, .btn-p'));
                nextBtn = btns.find(b => (b.innerText || '').toLowerCase().includes('next'));
            }
            if (nextBtn && nextBtn.offsetParent !== null && !nextBtn.disabled) {
                nextBtn.scrollIntoView({ behavior: 'smooth', block: 'center' });
                nextBtn.click();
                return { clicked: true, text: nextBtn.innerText.trim() };
            }
            return { clicked: false };
        })()
        "#;

        println!("[Worker #{wid}] [PR.Social] Menunggu 'Next task'...");
        for _attempt in 1..=25 {
            if self.check_account_not_found().await.unwrap_or(false) {
                anyhow::bail!("ACCOUNT_NOT_FOUND: We cannot find your Instagram account");
            }
            self.dismiss_too_fast_if_present().await;
            let res = self.pr_browser.evaluate(script).await?;
            if res.get("clicked").and_then(Value::as_bool) == Some(true) {
                println!("[Worker #{wid}] [PR.Social] 'Next task' diklik!");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(if self.fast_mode { 150 } else { 600 })).await;
        }

        anyhow::bail!("[Worker #{wid}] Timed out menunggu 'Next task'")
    }

    pub async fn check_safety_break_details(&self) -> Result<SafetyBreakDetails> {
        let script = r#"
        (() => {
            const text = (document.body ? document.body.innerText : '').toLowerCase();
            const headings = Array.from(document.querySelectorAll('h3, h2, h1, b, strong, p, div, span'));
            const hasSafetyBreak = headings.some(el => (el.innerText || '').toLowerCase().includes('a break to keep your accounts safe'))
                || text.includes('a break to keep your accounts safe')
                || text.includes('a break to keep your account');

            if (!hasSafetyBreak) {
                return { detected: false, untilMs: null, displayDuration: "", remainingSeconds: 0 };
            }

            const untilEl = document.querySelector('span[data-until], [data-until], [data-say="dur"]');
            let untilMs = null;
            let displayDuration = "";

            if (untilEl) {
                const rawUntil = untilEl.getAttribute('data-until');
                if (rawUntil) {
                    untilMs = parseFloat(rawUntil);
                }
                displayDuration = (untilEl.innerText || untilEl.textContent || '').trim();
            }

            const now = Date.now();
            let remainingSeconds = 0;
            if (untilMs && untilMs > now) {
                remainingSeconds = Math.round((untilMs - now) / 1000);
            } else if (displayDuration) {
                const match = displayDuration.match(/(\d+)\s*(?:min|m)/i);
                if (match) {
                    remainingSeconds = parseInt(match[1], 10) * 60;
                }
            }

            return {
                detected: true,
                untilMs: untilMs,
                displayDuration: displayDuration,
                remainingSeconds: remainingSeconds
            };
        })()
        "#;
        let res = self.pr_browser.evaluate(script).await?;
        let parsed: SafetyBreakDetails = serde_json::from_value(res).unwrap_or_default();
        Ok(parsed)
    }

    pub async fn check_safety_break(&self) -> Result<bool> {
        let details = self.check_safety_break_details().await?;
        Ok(details.detected)
    }

    pub async fn check_account_not_found(&self) -> Result<bool> {
        let script = r#"
        (() => {
            const rawHtml = (document.body ? document.body.innerHTML : '').toLowerCase();
            const rawText = (document.body ? (document.body.innerText || document.body.textContent || '') : '').toLowerCase();
            const cleanText = rawText.replace(/\s+/g, ' ');

            if (rawHtml.includes('cannot find your instagram') ||
                rawHtml.includes('cannot find your account') ||
                cleanText.includes('cannot find your instagram') ||
                cleanText.includes('cannot find your account') ||
                /(?:we )?can(?:not|\'t| not) find (?:your )?instagram/i.test(cleanText) ||
                /cannot find (?:your )?instagram/i.test(rawHtml)) {
                return true;
            }

            const targets = Array.from(document.querySelectorAll('b, strong, .alert, .note, .warn, .popup, .modal, [class*="error"], [class*="alert"]'));
            return targets.some(el => {
                const t = (el.innerText || el.textContent || '').toLowerCase().replace(/\s+/g, ' ');
                return t.includes('cannot find your instagram') || t.includes('cannot find your account');
            });
        })()
        "#;
        let res = self.pr_browser.evaluate(script).await?;
        Ok(res.as_bool().unwrap_or(false))
    }

    pub async fn check_instagram_checkpoint(&self) -> Result<bool> {
        let script = r#"
        (() => {
            const url = (location.href || '').toLowerCase();
            if (url.includes('/challenge/') || 
                url.includes('/two_factor') || 
                url.includes('/suspended') || 
                url.includes('/checkpoint/')) {
                return true;
            }

            const html = (document.body ? document.body.innerHTML : '').toLowerCase();
            const text = (document.body ? (document.body.innerText || document.body.textContent || '') : '').toLowerCase();

            return html.includes('suspicious activity') ||
                   html.includes('help us confirm that you own this account') ||
                   html.includes('confirm it\'s you') ||
                   html.includes('we noticed unusual activity') ||
                   html.includes('checkpoint_required') ||
                   html.includes('challenge_required') ||
                   html.includes('enter the 6-digit code') ||
                   html.includes('we limit how often you can do certain things') ||
                   text.includes('suspicious activity') ||
                   text.includes('confirm that you own this account') ||
                   text.includes('we noticed unusual activity') ||
                   text.includes('we limit how often');
        })()
        "#;
        let res = self.ig_browser.evaluate(script).await?;
        Ok(res.as_bool().unwrap_or(false))
    }

    async fn click_next_task_js(&self) -> Result<()> {
        self.pr_browser.evaluate(r#"
            (() => {
                const btn = document.querySelector('button[data-go="next"], [data-go="next"]');
                if (btn && !btn.disabled) btn.click();
            })()
        "#).await?;
        Ok(())
    }
}

/// Run multiple workers concurrently with desynchronized starts.
pub async fn run_multi_workers(
    endpoints: Vec<String>,
    min_delay_secs: u64,
    max_delay_secs: u64,
    max_tasks: Option<usize>,
    session_limit: Option<usize>,
    fast_mode: bool,
) -> Result<()> {
    let num_workers = endpoints.len();
    println!("Memulai {} worker paralel (fast_mode: {})...", num_workers, fast_mode);

    let mut handles = Vec::new();

    for (i, ep) in endpoints.into_iter().enumerate() {
        // Derive real worker ID from debugging port (e.g. 9222 -> #1, 9224 -> #3, 9225 -> #4)
        let port_parsed: usize = ep.split(':').last()
            .and_then(|p| p.trim_matches('/').parse().ok())
            .unwrap_or(9222);
        let worker_id = if port_parsed >= 9222 {
            port_parsed - 9221
        } else {
            i + 1
        };
        let initial_delay = if fast_mode { 0 } else { (i as u64) * 3 };
        let sl = session_limit;

        let handle = tokio::spawn(async move {
            match SocialAutomation::connect(worker_id, &ep, min_delay_secs, max_delay_secs, initial_delay, fast_mode).await {
                Ok(mut runner) => {
                    let remaining_target = max_tasks;
                    loop {
                        match runner.run_loop(remaining_target, sl).await {
                            Ok(LoopResult::Completed(_)) => {
                                println!("[Worker #{worker_id}] Target task tercapai. Selesai!");
                                break;
                            }
                            Ok(LoopResult::SafetyBreak(details)) => {
                                let mut remaining = details.remaining_seconds;
                                if remaining == 0 {
                                    remaining = 38 * 60;
                                }
                                let jitter = rand::rng().random_range(60..=120_u64);
                                let total_wait = remaining + jitter;
                                println!("\n🛑 [Worker #{worker_id}] TERKENA TIMEOUT: 'A break to keep your accounts safe'!");
                                println!("⏳ [Worker #{worker_id}] Durasi timeout: {} (sisa {} detik + jitter {} detik = total {:.1} menit).",
                                    details.display_duration, remaining, jitter, total_wait as f64 / 60.0);
                                println!("⏳ [Worker #{worker_id}] Worker menunggu hitung mundur sampai timeout selesai, lalu OTOMATIS LANJUT...");
                                countdown(worker_id, total_wait, 15).await;
                                println!("⏳ [Worker #{worker_id}] Waktu timeout selesai! Me-reload halaman PR.Social dan melanjutkan task...\n");

                                let _ = runner.pr_browser.bring_to_front().await;
                                let _ = runner.pr_browser.navigate("https://www.pr.social/tasks").await;
                                tokio::time::sleep(Duration::from_millis(2000)).await;

                                // Kurangi target jika ada batas max
                                if let Some(target) = remaining_target {
                                    if target <= runner.worker_id {
                                        break;
                                    }
                                }
                            }
                            Ok(LoopResult::AccountNotFound(msg)) => {
                                println!("\n❌ [Worker #{worker_id}] Berhenti permanen: {msg}. Menutup browser Worker #{worker_id}...");
                                crate::batch_orchestrator::close_chrome_at_port(port_parsed as u16).await;
                                println!("❌ [Worker #{worker_id}] Browser Worker #{worker_id} (Port {port_parsed}) telah ditutup.\n");
                                break;
                            }
                            Ok(LoopResult::InstagramCheckpoint(msg)) => {
                                println!("\n🛑 [Worker #{worker_id}] Berhenti: {msg}. Menutup browser Worker #{worker_id}...");
                                crate::batch_orchestrator::close_chrome_at_port(port_parsed as u16).await;
                                println!("🛑 [Worker #{worker_id}] Browser Worker #{worker_id} (Port {port_parsed}) telah ditutup.\n");
                                break;
                            }
                            Err(e) => {
                                eprintln!("[Worker #{worker_id}] Error: {e:#}");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[Worker #{worker_id}] Gagal terhubung ke {ep}: {e:#}");
                }
            }
        });

        handles.push(handle);
    }

    futures_util::future::join_all(handles).await;
    println!("Semua worker selesai.");
    Ok(())
}

fn weighted_delay(min: u64, max: u64) -> u64 {
    if min >= max {
        return min;
    }
    let a = rand::rng().random_range(min..=max);
    let b = rand::rng().random_range(min..=max);
    (a + b) / 2
}

async fn countdown(wid: usize, total_secs: u64, print_interval: u64) {
    for remaining in (1..=total_secs).rev() {
        if remaining % print_interval == 0 || remaining <= 5 {
            println!("  [Worker #{wid}] Countdown: {remaining}s ...");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
