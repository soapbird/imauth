use crate::queue::types::{AuthJob, RefreshJob, ValidateJob};
use crate::ImauthError;
use async_nats::jetstream::stream::Config as StreamConfig;
use async_nats::jetstream::Context;

pub struct NatsQueue {
    js: Context,
    _stream_name: String,
}

impl NatsQueue {
    pub async fn connect(url: &str, stream_name: &str) -> crate::Result<Self> {
        let client = async_nats::connect(url)
            .await
            .map_err(|e| ImauthError::Queue(format!("Failed to connect to NATS: {e}")))?;
        let js = async_nats::jetstream::new(client);

        // Create stream if it doesn't exist
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

    pub async fn publish_auth_job(
        &self,
        job: &AuthJob,
    ) -> crate::Result<()> {
        let payload = serde_json::to_vec(job)
            .map_err(|e| ImauthError::Queue(format!("JSON serialize error: {e}")))?;
        self.js
            .publish("imauth.v1.auth.login".to_string(), payload.into())
            .await
            .map_err(|e| ImauthError::Queue(format!("Publish error: {e}")))?;
        Ok(())
    }

    pub async fn publish_refresh_job(
        &self,
        job: &RefreshJob,
    ) -> crate::Result<()> {
        let payload = serde_json::to_vec(job)
            .map_err(|e| ImauthError::Queue(format!("JSON serialize error: {e}")))?;
        self.js
            .publish("imauth.v1.jobs.refresh".to_string(), payload.into())
            .await
            .map_err(|e| ImauthError::Queue(format!("Publish error: {e}")))?;
        Ok(())
    }

    pub async fn publish_validate_job(
        &self,
        job: &ValidateJob,
    ) -> crate::Result<()> {
        let payload = serde_json::to_vec(job)
            .map_err(|e| ImauthError::Queue(format!("JSON serialize error: {e}")))?;
        self.js
            .publish("imauth.v1.jobs.validate".to_string(), payload.into())
            .await
            .map_err(|e| ImauthError::Queue(format!("Publish error: {e}")))?;
        Ok(())
    }
}
