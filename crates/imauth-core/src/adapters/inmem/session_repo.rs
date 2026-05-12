use crate::domain::session::Session;
use crate::ports::repository::SessionRepository;
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct InMemorySessionRepository {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl InMemorySessionRepository {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionRepository for InMemorySessionRepository {
    async fn create(&self, session: Session) -> Result<Session> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    async fn get(&self, id: &str) -> Result<Option<Session>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(id).cloned())
    }

    async fn update(&self, session: &Session) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id);
        Ok(())
    }
}
