#![allow(dead_code)]

use crate::Result;
use crate::domain::queue_jobs::{AuthJob, RefreshJob, ValidateJob};
use crate::ports::queue::JobQueue;
use crate::ImauthError;
use async_nats::jetstream::stream::Config as StreamConfig;
use async_nats::jetstream::Context;
use async_trait::async_trait;
use serde::Serialize;

pub struct NatsJobQueue {
    js: Context,
    _stream_name: String,
}

impl NatsJobQueue {
    pub async fn connect(url: &str, stream_name: &str) -> Result<Self> {
        let client = async_nats::connect(url)
            .await
            .map_err(|e| ImauthError::Queue(format!("Failed to connect to NATS: {e}")))?;
        let js = async_nats::jetstream::new(client);

        let _ = js
            .create_stream(StreamConfig {
                name: stream_name.to_string(),
                subjects: vec![
                    "imauth.v1.auth.*".to_string(),
                    "imauth.v1.session.*".to_string(),
                    "imauth.v1.credential.*".to_string(),
                    "imauth.v1.jobs.*".to_string(),
                ],
                ..Default::default()
            })
            .await;

        Ok(Self {
            js,
            _stream_name: stream_name.to_string(),
        })
    }

    async fn publish_json<T: Serialize>(&self, subject: &'static str, job: &T) -> Result<()> {
        let payload = serde_json::to_vec(job)
            .map_err(|e| ImauthError::Queue(format!("JSON serialize error: {e}")))?;
        self.js
            .publish(subject, payload.into())
            .await
            .map_err(|e| ImauthError::Queue(format!("Publish error: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl JobQueue for NatsJobQueue {
    async fn publish_auth_job(&self, job: &AuthJob) -> Result<()> {
        self.publish_json("imauth.v1.auth.login", job).await
    }

    async fn publish_refresh_job(&self, job: &RefreshJob) -> Result<()> {
        self.publish_json("imauth.v1.jobs.refresh", job).await
    }

    async fn publish_validate_job(&self, job: &ValidateJob) -> Result<()> {
        self.publish_json("imauth.v1.jobs.validate", job).await
    }
}
