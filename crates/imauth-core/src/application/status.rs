use crate::domain::session::Session;
use crate::ports::repository::SessionRepository;
use crate::ImauthError;
use crate::Result;
use std::sync::Arc;

pub struct GetStatusUseCase {
    sessions: Arc<dyn SessionRepository>,
}

impl GetStatusUseCase {
    pub fn new(sessions: Arc<dyn SessionRepository>) -> Self {
        Self { sessions }
    }

    pub async fn execute(&self, session_id: &str) -> Result<Session> {
        self.sessions
            .get(session_id)
            .await?
            .ok_or_else(|| ImauthError::NotFound("Session not found".into()))
    }
}

pub struct CancelSessionUseCase {
    sessions: Arc<dyn SessionRepository>,
}

impl CancelSessionUseCase {
    pub fn new(sessions: Arc<dyn SessionRepository>) -> Self {
        Self { sessions }
    }

    pub async fn execute(&self, session_id: &str) -> Result<()> {
        self.sessions.delete(session_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::repository::MockSessionRepository;

    #[tokio::test]
    async fn get_status_returns_not_found_when_missing() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get().return_once(|_| Ok(None));
        let uc = GetStatusUseCase::new(Arc::new(repo));
        let err = uc.execute("nope").await.unwrap_err();
        assert!(matches!(err, ImauthError::NotFound(_)));
    }

    #[tokio::test]
    async fn cancel_calls_delete_with_id() {
        let mut repo = MockSessionRepository::new();
        repo.expect_delete()
            .withf(|id| id == "s1")
            .times(1)
            .returning(|_| Ok(()));
        let uc = CancelSessionUseCase::new(Arc::new(repo));
        uc.execute("s1").await.unwrap();
    }
}
