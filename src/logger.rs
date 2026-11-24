use anyhow::Result;
use chrono::Local;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[derive(Serialize)]
pub struct TaskRecord {
    pub timestamp: String,
    pub worker_id: usize,
    pub task_number: usize,
    pub instagram_url: String,
    pub action: String,
    pub duration_ms: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct TaskLogger {
    writer: BufWriter<std::fs::File>,
    log_path: PathBuf,
    pub tasks_logged: usize,
}

impl TaskLogger {
    pub fn new(worker_id: usize) -> Result<Self> {
        let log_dir = PathBuf::from("logs");
        fs::create_dir_all(&log_dir)?;

        let ts = Local::now().format("%Y-%m-%d_%H-%M-%S");
        let filename = format!("worker{worker_id}_{ts}.jsonl");
        let log_path = log_dir.join(&filename);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        println!("[Worker #{worker_id}] Log file: {}", log_path.display());

        Ok(Self {
            writer: BufWriter::new(file),
            log_path,
            tasks_logged: 0,
        })
    }

    pub fn log_task(&mut self, record: &TaskRecord) -> Result<()> {
        let json = serde_json::to_string(record)?;
        writeln!(self.writer, "{json}")?;
        self.writer.flush()?;
        self.tasks_logged += 1;
        Ok(())
    }

    pub fn path(&self) -> &PathBuf {
        &self.log_path
    }

    /// Write a final summary line at the end of a session.
    pub fn write_summary(&mut self, completed: usize, errors: usize, total_duration_secs: u64) -> Result<()> {
        let summary = serde_json::json!({
            "type": "session_summary",
            "timestamp": Local::now().to_rfc3339(),
            "tasks_completed": completed,
            "tasks_errored": errors,
            "total_duration_secs": total_duration_secs,
        });
        writeln!(self.writer, "{}", serde_json::to_string(&summary)?)?;
        self.writer.flush()?;
        Ok(())
    }
}

