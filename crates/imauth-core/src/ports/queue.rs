use crate::Result;
use crate::domain::queue_jobs::{AuthJob, RefreshJob, ValidateJob};
use async_trait::async_trait;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn publish_auth_job(&self, job: &AuthJob) -> Result<()>;
    async fn publish_refresh_job(&self, job: &RefreshJob) -> Result<()>;
    async fn publish_validate_job(&self, job: &ValidateJob) -> Result<()>;
}
