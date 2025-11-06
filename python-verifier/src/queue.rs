use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Cross-platform persistent job queue using sled
pub struct JobQueue {
    db: Arc<sled::Db>,
    sender: mpsc::Sender<JobCommand>,
    receiver: Arc<RwLock<mpsc::Receiver<JobCommand>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedJob {
    pub id: String,
    pub code: String,
    pub input: serde_json::Value,
    pub priority: u8,
    pub created_at: u64,
    pub retry_count: u8,
    pub max_retries: u8,
}

#[derive(Debug)]
pub enum JobCommand {
    Submit(QueuedJob),
}

impl JobQueue {
    /// Create persistent queue that works on all platforms
    pub fn new(path: &str) -> Result<Self> {
        let db = Arc::new(sled::open(path)?);
        let (tx, rx) = mpsc::channel(1000);

        Ok(Self {
            db,
            sender: tx,
            receiver: Arc::new(RwLock::new(rx)),
        })
    }

    /// Submit job to queue
    pub async fn submit(&self, job: QueuedJob) -> Result<String> {
        let job_id = job.id.clone();
        let key = format!("job:{}", job_id);
        let value = serde_json::to_vec(&job)?;

        self.db.insert(key.as_bytes(), value)?;
        self.sender.send(JobCommand::Submit(job)).await?;

        Ok(job_id)
    }

    /// Get next job
    pub async fn next(&self) -> Result<Option<QueuedJob>> {
        let mut receiver = self.receiver.write().await;

        match receiver.recv().await {
            Some(JobCommand::Submit(job)) => Ok(Some(job)),
            _ => Ok(None),
        }
    }

    /// Mark job complete
    pub async fn complete(&self, job_id: &str, result: serde_json::Value) -> Result<()> {
        let job_key = format!("job:{}", job_id);
        let result_key = format!("result:{}", job_id);

        self.db.insert(result_key.as_bytes(), serde_json::to_vec(&result)?)?;
        self.db.remove(job_key.as_bytes())?;

        Ok(())
    }

    /// Mark job failed
    pub async fn fail(&self, job_id: &str, error: &str) -> Result<()> {
        let key = format!("job:{}", job_id);

        if let Some(data) = self.db.get(key.as_bytes())? {
            let mut job: QueuedJob = serde_json::from_slice(&data)?;

            if job.retry_count < job.max_retries {
                job.retry_count += 1;
                self.db.insert(key.as_bytes(), serde_json::to_vec(&job)?)?;
                // Re-submit for retry
                self.sender.send(JobCommand::Submit(job)).await?;
            } else {
                let error_key = format!("error:{}", job_id);
                self.db.insert(error_key.as_bytes(), error.as_bytes())?;
                self.db.remove(key.as_bytes())?;
            }
        }

        Ok(())
    }

    /// Clean old completed jobs
    pub fn cleanup_old(&self, older_than_secs: u64) -> Result<usize> {
        let now = chrono::Utc::now().timestamp() as u64;
        let cutoff = now - older_than_secs;
        let mut deleted = 0;

        for item in self.db.scan_prefix(b"result:") {
            let (key, value) = item?;

            if let Ok(result) = serde_json::from_slice::<serde_json::Value>(&value) {
                if let Some(ts) = result.get("timestamp").and_then(|v| v.as_u64()) {
                    if ts < cutoff {
                        self.db.remove(&key)?;
                        deleted += 1;
                    }
                }
            }
        }

        Ok(deleted)
    }
}