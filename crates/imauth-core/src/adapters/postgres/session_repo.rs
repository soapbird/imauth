use crate::domain::session::{Session, SessionState};
use crate::ports::repository::SessionRepository;
use crate::Result;
use async_trait::async_trait;
use chrono::DateTime;
use sqlx::PgPool;
use std::str::FromStr;

pub struct PostgresSessionRepository {
    pool: PgPool,
}

impl PostgresSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for PostgresSessionRepository {
    async fn create(&self, session: Session) -> Result<Session> {
        sqlx::query(
            r#"
            INSERT INTO sessions (id, platform, status, message, requires_input, input_type, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(&session.id)
        .bind(&session.platform)
        .bind(session.state.as_str())
        .bind(&session.message)
        .bind(session.requires_input)
        .bind(&session.input_type)
        .bind(session.created_at.timestamp())
        .bind(session.updated_at.timestamp())
        .execute(&self.pool)
        .await?;
        Ok(session)
    }

    async fn get(&self, id: &str) -> Result<Option<Session>> {
        let row: Option<(String, String, String, Option<String>, bool, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT id, platform, status, message, requires_input, input_type, created_at, updated_at FROM sessions WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(
                id,
                platform,
                status,
                message,
                requires_input,
                input_type,
                created_at,
                updated_at,
            )| {
                Session {
                    id,
                    platform,
                    state: SessionState::from_str(&status).unwrap_or(SessionState::Idle),
                    message,
                    requires_input,
                    input_type,
                    created_at: DateTime::from_timestamp(created_at, 0).unwrap_or_default(),
                    updated_at: DateTime::from_timestamp(updated_at, 0).unwrap_or_default(),
                }
            },
        ))
    }

    async fn update(&self, session: &Session) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE sessions SET
                status = $2,
                message = $3,
                requires_input = $4,
                input_type = $5,
                updated_at = $6
            WHERE id = $1
            "#,
        )
        .bind(&session.id)
        .bind(session.state.as_str())
        .bind(&session.message)
        .bind(session.requires_input)
        .bind(&session.input_type)
        .bind(session.updated_at.timestamp())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::postgres::{init_pool, run_migrations};

    #[tokio::test]
    async fn session_crud_roundtrip() {
        let database_url = match std::env::var("PG_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!("Skipping postgres test: PG_DATABASE_URL not set");
                return;
            }
        };
        let pool = init_pool(&database_url).await.unwrap();
        run_migrations(&pool).await.unwrap();

        let repo = PostgresSessionRepository::new(pool);
        let session = Session::new("pg-test-id".to_string(), "instagram".to_string());

        // create
        let created = repo.create(session.clone()).await.unwrap();
        assert_eq!(created.id, "pg-test-id");
        assert_eq!(created.platform, "instagram");

        // get
        let fetched = repo.get("pg-test-id").await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.platform, "instagram");
        assert_eq!(fetched.state, SessionState::Idle);

        // update
        let mut updated = session.clone();
        updated.transition(SessionState::Connected, Some("ok".to_string()));
        repo.update(&updated).await.unwrap();

        let fetched = repo.get("pg-test-id").await.unwrap().unwrap();
        assert_eq!(fetched.state, SessionState::Connected);
        assert_eq!(fetched.message.as_deref(), Some("ok"));

        // delete
        repo.delete("pg-test-id").await.unwrap();
        let fetched = repo.get("pg-test-id").await.unwrap();
        assert!(fetched.is_none());
    }
}
