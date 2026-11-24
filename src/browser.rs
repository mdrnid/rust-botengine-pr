use crate::cdp::CdpClient;
use crate::stealth;
use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;

pub struct Browser {
    pub cdp: CdpClient,
}

impl Browser {
    pub async fn fetch_targets(endpoint: &str) -> Result<(String, Vec<Value>)> {
        let base = endpoint.trim_end_matches('/');
        let candidates = [
            base.to_string(),
            base.replace("127.0.0.1", "[::1]"),
            base.replace("127.0.0.1", "localhost"),
            base.replace("[::1]", "127.0.0.1"),
        ];

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?;

        for c_base in candidates {
            let url = format!("{c_base}/json/list");
            if let Ok(res) = client.get(&url).send().await {
                if let Ok(targets) = res.json::<Vec<Value>>().await {
                    return Ok((c_base, targets));
                }
            }
        }

        anyhow::bail!("Cannot reach debugging endpoint at {endpoint} (tried 127.0.0.1 and [::1]). Start Chrome/Brave with remote debugging enabled.");
    }

    pub async fn connect(endpoint: &str, target_filter: Option<&str>) -> Result<Self> {
        let (_base, targets) = Self::fetch_targets(endpoint).await?;

        let target = targets.iter().find(|target| {
            target.get("type").and_then(Value::as_str) == Some("page")
                && target_filter.map_or(true, |filter| {
                    target.get("url").and_then(Value::as_str).unwrap_or_default().contains(filter)
                })
        }).context("No matching browser tab found. Open a tab or change --target.")?;

        let ws = target.get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .context("Target has no WebSocket debugger URL")?;

        let cdp = CdpClient::connect(ws).await?;
        cdp.command("Page.enable", serde_json::json!({})).await?;
        cdp.command("Runtime.enable", serde_json::json!({})).await?;
        stealth::inject_stealth(&cdp).await?;
        Ok(Self { cdp })
    }

    pub async fn evaluate(&self, expression: &str) -> Result<Value> {
        self.cdp.evaluate(expression).await
    }

    pub async fn navigate(&self, url: &str) -> Result<()> {
        self.cdp.command("Page.navigate", serde_json::json!({ "url": url })).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let state = self.evaluate("document.readyState").await?;
            if matches!(state.as_str(), Some("interactive" | "complete")) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!("Navigation timed out");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn screenshot(&self, path: &str) -> Result<()> {
        let response = self.cdp.command(
            "Page.captureScreenshot",
            serde_json::json!({ "format": "png", "captureBeyondViewport": false }),
        ).await?;
        let encoded = response.get("data").and_then(Value::as_str)
            .context("Screenshot response has no data")?;
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    pub async fn bring_to_front(&self) -> Result<()> {
        self.cdp.command("Page.bringToFront", serde_json::json!({})).await?;
        Ok(())
    }

    pub async fn reload(&self) -> Result<()> {
        let _ = self.cdp.command("Page.reload", serde_json::json!({})).await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        tokio::time::sleep(Duration::from_millis(500)).await;
        loop {
            if let Ok(state) = self.evaluate("document.readyState").await {
                if matches!(state.as_str(), Some("interactive" | "complete")) {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Ok(())
    }

    pub async fn ensure_tab(endpoint: &str, target_filter: &str, default_url: &str) -> Result<Self> {
        if let Ok(browser) = Self::connect(endpoint, Some(target_filter)).await {
            return Ok(browser);
        }

        let (base, _) = Self::fetch_targets(endpoint).await?;
        let new_tab_url = format!("{}/json/new?{}", base.trim_end_matches('/'), default_url);
        let _ = reqwest::Client::new().put(&new_tab_url).send().await;
        tokio::time::sleep(Duration::from_millis(800)).await;

        Self::connect(endpoint, Some(target_filter)).await
    }
}
