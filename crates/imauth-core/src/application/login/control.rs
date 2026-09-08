use super::LoginEvent;
use crate::ports::repository::SessionRepository;
use crate::ImauthError;
use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(super) const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub(super) enum LoginFailure {
    #[error("Login cancelled")]
    Cancelled,
    #[error("Login timed out during {0}")]
    TimedOut(&'static str),
    #[error(transparent)]
    Operation(#[from] ImauthError),
}

pub(super) struct LoginControl<'a> {
    pub sessions: &'a dyn SessionRepository,
    pub session_id: &'a str,
    pub tx: &'a mpsc::Sender<LoginEvent>,
    pub deadline: Instant,
    pub stage: &'static str,
}

impl LoginControl<'_> {
    pub async fn run<T>(
        &self,
        operation: impl Future<Output = crate::Result<T>>,
    ) -> Result<T, LoginFailure> {
        tokio::select! {
            biased;
            _ = self.tx.closed() => Err(LoginFailure::Cancelled),
            _ = tokio::time::sleep_until(self.deadline) => Err(LoginFailure::TimedOut(self.stage)),
            result = operation => result.map_err(LoginFailure::from),
            _ = self.wait_cancelled() => Err(LoginFailure::Cancelled),
        }
    }

    async fn wait_cancelled(&self) {
        loop {
            tokio::time::sleep(CANCELLATION_POLL_INTERVAL).await;
            match self.sessions.get(self.session_id).await {
                Ok(None) => return,
                Ok(Some(_)) => {}
                Err(error) => tracing::warn!(%error, "failed to check login cancellation"),
            }
        }
    }
}
