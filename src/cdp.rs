use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::{oneshot, Mutex},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::Message,
    MaybeTlsStream, WebSocketStream,
};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone)]
pub struct CdpClient {
    writer: Arc<Mutex<futures_util::stream::SplitSink<Ws, Message>>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>,
    next_id: Arc<AtomicU64>,
}

impl CdpClient {
    pub async fn connect(ws_url: &str) -> Result<Self> {
        let (socket, _) = connect_async(ws_url).await.context("CDP WebSocket connection failed")?;
        let (writer, mut reader) = socket.split();

        let client = Self {
            writer: Arc::new(Mutex::new(writer)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        };

        let pending = Arc::clone(&client.pending);
        tokio::spawn(async move {
            while let Some(message) = reader.next().await {
                let Ok(message) = message else { break };
                let Message::Text(text) = message else { continue };
                let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                let Some(id) = value.get("id").and_then(Value::as_u64) else { continue };

                if let Some(tx) = pending.lock().await.remove(&id) {
                    let result = if let Some(error) = value.get("error") {
                        Err(error.to_string())
                    } else {
                        Ok(value.get("result").cloned().unwrap_or_else(|| json!({})))
                    };
                    let _ = tx.send(result);
                }
            }

            let mut pending = pending.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err("CDP WebSocket closed".to_string()));
            }
        });

        Ok(client)
    }

    pub async fn command(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let payload = json!({ "id": id, "method": method, "params": params });
        if let Err(error) = self.writer.lock().await.send(Message::Text(payload.to_string().into())).await {
            self.pending.lock().await.remove(&id);
            return Err(error).context("Could not send CDP command");
        }

        let response = tokio::time::timeout(Duration::from_secs(25), rx)
            .await.context("CDP command timed out")?
            .context("CDP response channel closed")?;

        match response {
            Ok(value) => Ok(value),
            Err(error) => bail!("CDP {method} error: {error}"),
        }
    }

    pub async fn evaluate(&self, expression: &str) -> Result<Value> {
        let response = self.command(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true
            }),
        ).await?;

        if let Some(exception) = response.get("exceptionDetails") {
            bail!("JavaScript exception: {exception}");
        }

        let remote = response.get("result").cloned().unwrap_or_else(|| json!({}));
        Ok(remote.get("value").cloned().unwrap_or(remote))
    }
}
