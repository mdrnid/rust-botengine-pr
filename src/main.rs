mod batch_orchestrator;
mod browser;
mod cdp;
mod input;
mod inspector;
mod logger;
mod social_task;
mod stealth;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "browser-click", version, about = "Manual coordinate-based browser automation over Chrome CDP")]
struct Cli {
    /// Local Chrome/Brave/Chromium debugging endpoint
    #[arg(long, global = true, default_value = "http://127.0.0.1:9222")]
    endpoint: String,

    /// Select a page target whose URL contains this value
    #[arg(long, global = true)]
    target: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Show selected tab information
    Info,

    /// Inspect page DOM and discover interactive elements
    Inspect {
        /// CSS selector to query specific elements (e.g. 'button', '#submit', '.btn')
        #[arg(short, long)]
        selector: Option<String>,

        /// Inspect all DOM elements instead of only interactive elements
        #[arg(long, default_value_t = false)]
        all: bool,

        /// Only include visible elements (bounding box > 0, visible style)
        #[arg(long, default_value_t = false)]
        visible_only: bool,

        /// Filter elements by text, tag, id, class, or aria-label (case-insensitive)
        #[arg(short, long)]
        filter: Option<String>,

        /// Limit number of returned elements
        #[arg(short, long)]
        limit: Option<usize>,

        /// Inspect a specific element by its index
        #[arg(long)]
        index: Option<usize>,

        /// Output compact single-line JSON instead of pretty-printed JSON
        #[arg(long, default_value_t = false)]
        compact: bool,
    },

    /// Navigate selected tab
    Open { url: String },

    /// Click at CSS viewport coordinates (origin: top-left of webpage viewport)
    Click {
        x: f64,
        y: f64,
        #[arg(long, default_value_t = 1)]
        count: u8,
    },

    /// Click an element found by DOM inspection using its index
    ClickElement {
        /// Element index as returned by inspect
        index: usize,

        /// Number of clicks (1 for single click, 2 for double click)
        #[arg(long, default_value_t = 1)]
        count: u8,
    },

    /// Move pointer to CSS viewport coordinates without clicking
    Move { x: f64, y: f64 },

    /// Scroll the page by CSS pixel deltas
    Scroll {
        delta_y: f64,
        #[arg(long, default_value_t = 0.0)]
        delta_x: f64,
        #[arg(long, default_value_t = 500.0)]
        x: f64,
        #[arg(long, default_value_t = 500.0)]
        y: f64,
    },

    /// Type text using browser keyboard input
    Type { text: String },

    /// Press a common key: ENTER, TAB, ESC, BACKSPACE, SPACE
    Key { key: String },

    /// Save a screenshot of the current browser viewport
    Screenshot { path: String },

    /// Run automated PR.Social Instagram follow loop
    AutoFollow {
        /// Number of concurrent workers (default: 1)
        #[arg(short, long, default_value_t = 1)]
        workers: usize,

        /// Base port for workers (default: 9222, e.g. 9222, 9223, 9224, 9225)
        #[arg(long, default_value_t = 9222)]
        base_port: u16,

        /// Custom ports list (comma-separated, overrides --workers and --base-port, e.g. "9222,9223")
        #[arg(long)]
        ports: Option<String>,

        /// Minimum delay in seconds between tasks (default: 8)
        #[arg(long, default_value_t = 8)]
        min_delay: u64,

        /// Maximum delay in seconds between tasks (default: 25)
        #[arg(long, default_value_t = 25)]
        max_delay: u64,

        /// Maximum number of tasks to complete (default: unlimited)
        #[arg(short, long)]
        max: Option<usize>,

        /// Session limit: rest 15-30 min after this many tasks (default: 80, 0 = no limit)
        #[arg(long, default_value_t = 80)]
        session_limit: usize,

        /// Fast mode: disable random jitter, profile scrolling, deliberate pauses, and cooldowns
        #[arg(short, long, default_value_t = false)]
        fast: bool,
    },

    /// Run multi-batch rotation system (default: 4 batches, 2 workers per batch, 50 tasks each)
    BatchFollow {
        /// Base debugging port (default: 9222 -> Batch 1: 9222-9223, Batch 2: 9224-9225, etc.)
        #[arg(long, default_value_t = 9222)]
        base_port: u16,

        /// Number of batches (default: 4)
        #[arg(short, long, default_value_t = 4)]
        batches: usize,

        /// Custom list of active batches (comma-separated, e.g. "2,3" or "2,3,4" to skip batch 1)
        #[arg(long)]
        active_batches: Option<String>,

        /// Number of concurrent workers per batch (default: 2)
        #[arg(short, long, default_value_t = 2)]
        workers_per_batch: usize,

        /// Number of tasks per worker in each batch before switching (default: 50)
        #[arg(short, long, default_value_t = 50)]
        tasks: usize,

        /// Total rotation cycles to run (default: unlimited)
        #[arg(short, long)]
        cycles: Option<usize>,

        /// Minimum extra jitter in seconds when safety break timeout is encountered (default: 60)
        #[arg(long, default_value_t = 60)]
        min_break_jitter: u64,

        /// Maximum extra jitter in seconds when safety break timeout is encountered (default: 120)
        #[arg(long, default_value_t = 120)]
        max_break_jitter: u64,

        /// Keep browsers open in background instead of closing inactive batches
        #[arg(long, default_value_t = false)]
        keep_open: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Command::AutoFollow {
        workers,
        base_port,
        ports,
        mut min_delay,
        mut max_delay,
        max,
        session_limit,
        fast,
    } = cli.command {
        // In fast mode, default to optimal 3-6s delay with jitter to prevent velocity flags
        if fast && min_delay == 8 && max_delay == 25 {
            min_delay = 3;
            max_delay = 6;
        }

        let endpoint_list: Vec<String> = if let Some(p_str) = ports {
            p_str
                .split(',')
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .map(|p| format!("http://127.0.0.1:{p}"))
                .collect()
        } else {
            let target_count = if workers > 1 { workers } else { 1 };
            let default_endpoints: Vec<String> = (0..target_count)
                .map(|i| format!("http://127.0.0.1:{}", base_port + i as u16))
                .collect();

            // Quick check apakah endpoint default (9222, 9223) sedang online
            let mut all_default_online = true;
            for ep in &default_endpoints {
                if browser::Browser::fetch_targets(ep).await.is_err() {
                    all_default_online = false;
                    break;
                }
            }

            if all_default_online {
                default_endpoints
            } else {
                // Auto-Discovery Pintar: memindai port 9222..9237 untuk mencari browser Chrome yang SEDANG AKTIF
                println!("🔍 Memindai browser Chrome yang sedang aktif di komputer...");
                let mut discovered = Vec::new();
                for p in 9222..=9237 {
                    let ep = format!("http://127.0.0.1:{p}");
                    if let Ok((_base, targets)) = browser::Browser::fetch_targets(&ep).await {
                        let has_page = targets.iter().any(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"));
                        if has_page {
                            let wid = p - 9221;
                            println!("   [ONLINE] Menemukan Chrome aktif pada Port {p} (Worker #{wid})");
                            discovered.push(ep);
                            if discovered.len() >= target_count {
                                break;
                            }
                        }
                    }
                }

                if !discovered.is_empty() {
                    println!("🎯 Otomatis memilih {} worker yang sedang terbuka di layar: {:?}\n", discovered.len(), discovered);
                    discovered
                } else {
                    default_endpoints
                }
            }
        };

        let sl = if fast || session_limit == 0 { None } else { Some(session_limit) };

        println!("Menjalankan {} worker pada endpoint: {:?}", endpoint_list.len(), endpoint_list);
        return social_task::run_multi_workers(endpoint_list, min_delay, max_delay, max, sl, fast).await;
    }

    if let Command::BatchFollow {
        base_port,
        batches,
        active_batches,
        workers_per_batch,
        tasks,
        cycles,
        min_break_jitter,
        max_break_jitter,
        keep_open,
    } = cli.command {
        return batch_orchestrator::run_batch_system(
            base_port,
            batches,
            active_batches,
            workers_per_batch,
            tasks,
            cycles,
            min_break_jitter,
            max_break_jitter,
            !keep_open,
        ).await;
    }

    let browser = browser::Browser::connect(&cli.endpoint, cli.target.as_deref()).await?;

    match cli.command {
        Command::Info => {
            let info = browser.evaluate(
                "JSON.stringify({title:document.title,url:location.href,viewport:{width:innerWidth,height:innerHeight},readyState:document.readyState})"
            ).await?;
            println!("{}", info.as_str().unwrap_or(&info.to_string()));
        }
        Command::Inspect {
            selector,
            all,
            visible_only,
            filter,
            limit,
            index,
            compact,
        } => {
            let options = inspector::InspectOptions {
                selector,
                all,
                visible_only,
                filter,
                limit,
                index,
            };
            let result = inspector::inspect(&browser, &options).await?;
            if compact {
                println!("{}", serde_json::to_string(&result)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        }
        Command::Open { url } => {
            browser.navigate(&url).await?;
            println!("Opened: {url}");
        }
        Command::Click { x, y, count } => {
            if count == 0 || count > 2 {
                anyhow::bail!("count must be 1 or 2");
            }
            input::click(&browser, x, y, count).await?;
            println!("Clicked viewport coordinate ({x}, {y})");
        }
        Command::ClickElement { index, count } => {
            if count == 0 || count > 2 {
                anyhow::bail!("count must be 1 or 2");
            }
            let options = inspector::InspectOptions {
                index: Some(index),
                ..Default::default()
            };
            let result = inspector::inspect(&browser, &options).await?;
            let el = result.elements.first().with_context(|| format!("Element with index {index} not found"))?;
            if !el.visible {
                anyhow::bail!("Element with index {index} (<{}>) is not visible", el.tag);
            }
            let x = el.rect.x + (el.rect.width / 2.0);
            let y = el.rect.y + (el.rect.height / 2.0);
            input::click(&browser, x, y, count).await?;
            println!("Clicked element #{index} <{}> at viewport center ({x:.1}, {y:.1})", el.tag);
        }
        Command::Move { x, y } => {
            input::move_to(&browser, x, y).await?;
            println!("Moved pointer to viewport coordinate ({x}, {y})");
        }
        Command::Scroll { delta_y, delta_x, x, y } => {
            input::scroll(&browser, x, y, delta_x, delta_y).await?;
            println!("Scrolled: delta_x={delta_x}, delta_y={delta_y}");
        }
        Command::Type { text } => {
            input::type_text(&browser, &text).await?;
            println!("Typed {} characters", text.chars().count());
        }
        Command::Key { key } => {
            input::press_key(&browser, &key).await?;
            println!("Pressed: {}", key.to_uppercase());
        }
        Command::Screenshot { path } => {
            browser.screenshot(&path).await?;
            println!("Saved screenshot: {path}");
        }
        Command::AutoFollow { .. } => unreachable!(),
        Command::BatchFollow { .. } => unreachable!(),
    }

    Ok(())
}
