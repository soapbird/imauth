use crate::domain::RefreshToken;
use crate::ports::repository::RefreshTokenRepository;
use crate::Result;
use async_trait::async_trait;
use chrono::DateTime;
use sqlx::PgPool;

pub struct PostgresRefreshTokenRepository {
    pool: PgPool,
}

impl PostgresRefreshTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RefreshTokenRepository for PostgresRefreshTokenRepository {
    async fn save(&self, token: &RefreshToken) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO refresh_tokens (platform, token_encrypted, expires_at, last_refreshed_at, updated_at)
            VALUES ($1, $2, $3, $4, EXTRACT(EPOCH FROM NOW())::bigint)
            ON CONFLICT(platform) DO UPDATE SET
                token_encrypted = excluded.token_encrypted,
                expires_at = excluded.expires_at,
                last_refreshed_at = excluded.last_refreshed_at,
                updated_at = EXTRACT(EPOCH FROM NOW())::bigint
            "#,
        )
        .bind(&token.platform)
        .bind(&token.token_encrypted)
        .bind(token.expires_at.map(|dt| dt.timestamp()))
        .bind(token.last_refreshed_at.map(|dt| dt.timestamp()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, platform: &str) -> Result<Option<RefreshToken>> {
        let row: Option<(String, String, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT platform, token_encrypted, expires_at, last_refreshed_at FROM refresh_tokens WHERE platform = $1"
        )
        .bind(platform)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(platform, token_encrypted, expires_at, last_refreshed_at)| RefreshToken {
                platform,
                token_encrypted,
                expires_at: expires_at.and_then(|ts| DateTime::from_timestamp(ts, 0)),
                last_refreshed_at: last_refreshed_at.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            },
        ))
    }
}
