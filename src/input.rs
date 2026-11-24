use crate::browser::Browser;
use anyhow::Result;
use rand::Rng;
use serde_json::json;
use std::time::Duration;

fn validate_point(browser_info: &serde_json::Value, x: f64, y: f64) -> Result<()> {
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        anyhow::bail!("Coordinates must be finite, non-negative viewport pixels");
    }
    let width = browser_info["viewport"]["width"].as_f64().unwrap_or(0.0);
    let height = browser_info["viewport"]["height"].as_f64().unwrap_or(0.0);
    if width > 0.0 && height > 0.0 && (x >= width || y >= height) {
        anyhow::bail!("Coordinates are outside the current viewport ({width} x {height})");
    }
    Ok(())
}

async fn check_point(browser: &Browser, x: f64, y: f64) -> Result<()> {
    let info = browser.evaluate(
        "JSON.stringify({viewport:{width:innerWidth,height:innerHeight}})"
    ).await?;
    let parsed: serde_json::Value = if let Some(s) = info.as_str() {
        serde_json::from_str(s)?
    } else {
        info
    };
    validate_point(&parsed, x, y)
}

pub async fn move_to(browser: &Browser, x: f64, y: f64) -> Result<()> {
    check_point(browser, x, y).await?;
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mouseMoved", "x": x, "y": y
    })).await?;
    Ok(())
}

pub async fn click(browser: &Browser, x: f64, y: f64, count: u8) -> Result<()> {
    check_point(browser, x, y).await?;
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mouseMoved", "x": x, "y": y
    })).await?;

    for click_count in 1..=count {
        browser.cdp.command("Input.dispatchMouseEvent", json!({
            "type": "mousePressed",
            "x": x, "y": y,
            "button": "left",
            "clickCount": click_count
        })).await?;
        browser.cdp.command("Input.dispatchMouseEvent", json!({
            "type": "mouseReleased",
            "x": x, "y": y,
            "button": "left",
            "clickCount": click_count
        })).await?;
    }
    Ok(())
}

pub async fn scroll(browser: &Browser, x: f64, y: f64, delta_x: f64, delta_y: f64) -> Result<()> {
    check_point(browser, x, y).await?;
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mouseWheel",
        "x": x, "y": y,
        "deltaX": delta_x,
        "deltaY": delta_y
    })).await?;
    Ok(())
}

pub async fn type_text(browser: &Browser, text: &str) -> Result<()> {
    browser.cdp.command("Input.insertText", json!({ "text": text })).await?;
    Ok(())
}

/// Cubic Bézier interpolation: B(t) = (1-t)³P0 + 3(1-t)²tP1 + 3(1-t)t²P2 + t³P3
fn bezier(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

/// Move mouse along a human-like Bézier curve to the target position.
/// Starts from a random nearby position to simulate natural hand movement.
pub async fn human_move_to(browser: &Browser, target_x: f64, target_y: f64) -> Result<()> {
    // Generate all random values upfront (ThreadRng is !Send, can't hold across .await)
    let offset_range = 180.0_f64;
    let start_x = (target_x + rand::rng().random_range(-offset_range..=offset_range)).max(5.0);
    let start_y = (target_y + rand::rng().random_range(-offset_range * 0.7..=offset_range * 0.7)).max(5.0);
    let cp1_x = start_x + rand::rng().random_range(-90.0..=90.0);
    let cp1_y = start_y + rand::rng().random_range(-70.0..=70.0);
    let cp2_x = target_x + rand::rng().random_range(-45.0..=45.0);
    let cp2_y = target_y + rand::rng().random_range(-35.0..=35.0);
    let steps: u32 = rand::rng().random_range(18..=28);

    // Pre-compute all step data to avoid holding rng across awaits
    let mut moves: Vec<(f64, f64, u64)> = Vec::with_capacity(steps as usize + 1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let eased = 1.0 - (1.0 - t).powi(2); // ease-out: fast start, slow finish

        let x = bezier(eased, start_x, cp1_x, cp2_x, target_x)
            + rand::rng().random_range(-1.5..=1.5);
        let y = bezier(eased, start_y, cp1_y, cp2_y, target_y)
            + rand::rng().random_range(-1.5..=1.5);
        let delay_ms = rand::rng().random_range(6..=22);

        moves.push((x.max(0.0), y.max(0.0), delay_ms));
    }

    for (x, y, delay_ms) in moves {
        browser.cdp.command("Input.dispatchMouseEvent", json!({
            "type": "mouseMoved", "x": x, "y": y
        })).await?;
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }

    Ok(())
}

/// Click with human-like mouse movement to a random point within the bounding rect.
/// Simulates natural targeting: cursor approaches via Bézier curve, hovers briefly, then clicks.
pub async fn human_click(browser: &Browser, rect_x: f64, rect_y: f64, rect_w: f64, rect_h: f64) -> Result<()> {
    // Target a random point within the element (not dead center)
    let margin_x = (rect_w * 0.15).max(2.0);
    let margin_y = (rect_h * 0.15).max(2.0);
    let click_x = rand::rng().random_range((rect_x + margin_x)..=(rect_x + rect_w - margin_x));
    let click_y = rand::rng().random_range((rect_y + margin_y)..=(rect_y + rect_h - margin_y));
    let hover_ms = rand::rng().random_range(80..=200);

    // Move mouse to target with Bézier curve
    human_move_to(browser, click_x, click_y).await?;

    // Hover pause before clicking
    tokio::time::sleep(Duration::from_millis(hover_ms)).await;

    // Click
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mousePressed", "x": click_x, "y": click_y, "button": "left", "clickCount": 1
    })).await?;
    let release_delay = rand::rng().random_range(40..=120);
    tokio::time::sleep(Duration::from_millis(release_delay)).await;
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mouseReleased", "x": click_x, "y": click_y, "button": "left", "clickCount": 1
    })).await?;

    Ok(())
}

/// Scroll the page in human-like increments with variable speed.
pub async fn human_scroll(browser: &Browser, total_delta_y: f64) -> Result<()> {
    let steps: u32 = rand::rng().random_range(3..=7);
    let base_per_step = total_delta_y / steps as f64;

    // Pre-compute scroll events
    let mut events: Vec<(f64, f64, f64, u64)> = Vec::with_capacity(steps as usize);
    for _ in 0..steps {
        let jitter = rand::rng().random_range(-15.0..=15.0);
        let x = rand::rng().random_range(300.0..=800.0);
        let y = rand::rng().random_range(250.0..=500.0);
        let delay = rand::rng().random_range(80..=250);
        events.push((x, y, base_per_step + jitter, delay));
    }

    for (x, y, dy, delay) in events {
        browser.cdp.command("Input.dispatchMouseEvent", json!({
            "type": "mouseWheel", "x": x, "y": y, "deltaX": 0.0, "deltaY": dy
        })).await?;
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    Ok(())
}

/// Fast hardware CDP click with small random offset and brief hover.
/// Takes only ~120-180ms. Produces genuine isTrusted: true hardware mouse events.
pub async fn fast_cdp_click(browser: &Browser, rect_x: f64, rect_y: f64, rect_w: f64, rect_h: f64) -> Result<()> {
    let margin_x = (rect_w * 0.2).max(2.0);
    let margin_y = (rect_h * 0.2).max(2.0);
    let click_x = rand::rng().random_range((rect_x + margin_x)..=(rect_x + rect_w - margin_x));
    let click_y = rand::rng().random_range((rect_y + margin_y)..=(rect_y + rect_h - margin_y));

    // 1. Move to target point
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mouseMoved", "x": click_x, "y": click_y
    })).await?;

    let hover_ms = rand::rng().random_range(40..=90);
    tokio::time::sleep(Duration::from_millis(hover_ms)).await;

    // 2. Mouse pressed
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mousePressed", "x": click_x, "y": click_y, "button": "left", "clickCount": 1
    })).await?;

    let press_ms = rand::rng().random_range(35..=80);
    tokio::time::sleep(Duration::from_millis(press_ms)).await;

    // 3. Mouse released
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mouseReleased", "x": click_x, "y": click_y, "button": "left", "clickCount": 1
    })).await?;

    Ok(())
}

/// Instant natural micro-scroll to trigger scroll & viewport activity
pub async fn fast_micro_scroll(browser: &Browser, delta_y: f64) -> Result<()> {
    let x = rand::rng().random_range(350.0..=650.0);
    let y = rand::rng().random_range(200.0..=400.0);
    browser.cdp.command("Input.dispatchMouseEvent", json!({
        "type": "mouseWheel", "x": x, "y": y, "deltaX": 0.0, "deltaY": delta_y
    })).await?;
    Ok(())
}

pub async fn press_key(browser: &Browser, key: &str) -> Result<()> {
    let (key, code, windows_vk) = match key.to_ascii_uppercase().as_str() {
        "ENTER" => ("Enter", "Enter", 13),
        "TAB" => ("Tab", "Tab", 9),
        "ESC" | "ESCAPE" => ("Escape", "Escape", 27),
        "BACKSPACE" => ("Backspace", "Backspace", 8),
        "SPACE" => (" ", "Space", 32),
        _ => anyhow::bail!("Unsupported key. Supported: ENTER, TAB, ESC, BACKSPACE, SPACE"),
    };

    browser.cdp.command("Input.dispatchKeyEvent", json!({
        "type": "keyDown",
        "key": key,
        "code": code,
        "windowsVirtualKeyCode": windows_vk
    })).await?;
    browser.cdp.command("Input.dispatchKeyEvent", json!({
        "type": "keyUp",
        "key": key,
        "code": code,
        "windowsVirtualKeyCode": windows_vk
    })).await?;
    Ok(())
}
