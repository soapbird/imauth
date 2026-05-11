#![allow(dead_code)]

use crate::Result;
use crate::domain::session::{Session, SessionState};
use crate::ports::repository::SessionRepository;
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
                    requires_input: requires_input != 0,
                    input_type,
                    cookies: Vec::new(),
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
