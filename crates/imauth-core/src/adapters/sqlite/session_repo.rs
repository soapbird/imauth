use crate::domain::session::{Session, SessionState};
use crate::ports::repository::SessionRepository;
use crate::{ImauthError, Result};
use async_trait::async_trait;
use chrono::DateTime;
use sqlx::SqlitePool;
use std::str::FromStr;

pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn create(&self, session: Session) -> Result<Session> {
        sqlx::query(
            r#"
            INSERT INTO sessions (id, platform, status, message, requires_input, input_type, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&session.id)
        .bind(&session.platform)
        .bind(session.state.as_str())
        .bind(&session.message)
        .bind(session.requires_input as i32)
        .bind(&session.input_type)
        .bind(session.created_at.timestamp())
        .bind(session.updated_at.timestamp())
        .execute(&self.pool)
        .await?;
        Ok(session)
    }

    async fn get(&self, id: &str) -> Result<Option<Session>> {
        let row: Option<(String, String, String, Option<String>, i32, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT id, platform, status, message, requires_input, input_type, created_at, updated_at FROM sessions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(
            |(
                id,
                platform,
                status,
                message,
                requires_input,
                input_type,
                created_at,
                updated_at,
            )|
             -> Result<Session> {
                let state = SessionState::from_str(&status).map_err(ImauthError::Database)?;
                Ok(Session {
                    id,
                    platform,
                    state,
                    message,
                    requires_input: requires_input != 0,
                    input_type,
                    created_at: DateTime::from_timestamp(created_at, 0).unwrap_or_default(),
                    updated_at: DateTime::from_timestamp(updated_at, 0).unwrap_or_default(),
                })
            },
        )
        .transpose()
    }

    async fn update(&self, session: &Session) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE sessions SET
                status = ?2,
                message = ?3,
                requires_input = ?4,
                input_type = ?5,
                updated_at = ?6
            WHERE id = ?1
            "#,
        )
        .bind(&session.id)
        .bind(session.state.as_str())
        .bind(&session.message)
        .bind(session.requires_input as i32)
        .bind(&session.input_type)
        .bind(session.updated_at.timestamp())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite::run_migrations;

    async fn test_repository() -> SqliteSessionRepository {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();
        SqliteSessionRepository::new(pool)
    }

    fn session() -> Session {
        let created_at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let updated_at = DateTime::from_timestamp(1_700_000_123, 0).unwrap();
        Session {
            id: "session-1".to_string(),
            platform: "instagram".to_string(),
            state: SessionState::WaitingForUser,
            message: Some("waiting for login".to_string()),
            requires_input: true,
            input_type: Some("viewer_url".to_string()),
            created_at,
            updated_at,
        }
    }

    #[tokio::test]
    async fn get_round_trips_created_session_fields() {
        // Given: a fully populated session persisted in a fresh repository.
        let repo = test_repository().await;
        let expected = session();
        repo.create(expected.clone()).await.unwrap();

        // When: the session is loaded by ID.
        let actual = repo.get(&expected.id).await.unwrap().unwrap();

        // Then: all persisted state, input, and timestamp fields round-trip.
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.platform, expected.platform);
        assert_eq!(actual.state, expected.state);
        assert_eq!(actual.message, expected.message);
        assert_eq!(actual.requires_input, expected.requires_input);
        assert_eq!(actual.input_type, expected.input_type);
        assert_eq!(actual.created_at, expected.created_at);
        assert_eq!(actual.updated_at, expected.updated_at);
    }

    #[tokio::test]
    async fn update_persists_mutable_session_fields() {
        // Given: an idle session persisted in a fresh repository.
        let repo = test_repository().await;
        let mut expected = session();
        expected.state = SessionState::Idle;
        expected.message = None;
        expected.requires_input = false;
        expected.input_type = None;
        repo.create(expected.clone()).await.unwrap();
        expected.state = SessionState::Failed;
        expected.message = Some("browser unavailable".to_string());
        expected.updated_at = DateTime::from_timestamp(1_700_000_456, 0).unwrap();

        // When: the session is updated.
        repo.update(&expected).await.unwrap();

        // Then: the changed state and timestamp are observable on reload.
        let actual = repo.get(&expected.id).await.unwrap().unwrap();
        assert_eq!(actual.state, SessionState::Failed);
        assert_eq!(actual.message.as_deref(), Some("browser unavailable"));
        assert!(!actual.requires_input);
        assert_eq!(actual.input_type, None);
        assert_eq!(actual.updated_at, expected.updated_at);
        assert_eq!(actual.created_at, expected.created_at);
    }

    #[tokio::test]
    async fn delete_removes_existing_session() {
        // Given: an existing session in a fresh repository.
        let repo = test_repository().await;
        let existing = session();
        repo.create(existing.clone()).await.unwrap();

        // When: the session is deleted.
        repo.delete(&existing.id).await.unwrap();

        // Then: subsequent lookup reports no session.
        assert!(repo.get(&existing.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_rejects_unknown_persisted_session_state() {
        // Given: a fresh repository containing a corrupt persisted state.
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, platform, status, created_at, updated_at) \
             VALUES ('session-1', 'instagram', 'corrupt', 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let repo = SqliteSessionRepository::new(pool);

        // When: the corrupt session is loaded.
        let error = repo.get("session-1").await.unwrap_err();

        // Then: the adapter rejects the unknown domain state.
        assert!(
            matches!(error, ImauthError::Database(message) if message == "unknown session state: corrupt")
        );
    }
}
