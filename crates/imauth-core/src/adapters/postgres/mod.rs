pub mod cookie_repo;
pub mod credential_repo;
pub mod refresh_repo;
pub mod session_repo;

pub use cookie_repo::PostgresCookieRepository;
pub use credential_repo::PostgresCredentialRepository;
pub use refresh_repo::PostgresRefreshTokenRepository;
pub use session_repo::PostgresSessionRepository;

use crate::ImauthError;
use crate::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn init_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(|e| ImauthError::Database(format!("Failed to connect to Postgres: {e}")))?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            platform TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'idle',
            message TEXT,
            requires_input BOOLEAN NOT NULL DEFAULT FALSE,
            input_type TEXT,
            created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint,
            updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            platform TEXT PRIMARY KEY,
            token_encrypted TEXT NOT NULL,
            expires_at BIGINT,
            last_refreshed_at BIGINT,
            created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint,
            updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cookies (
            id BIGSERIAL PRIMARY KEY,
            platform TEXT NOT NULL,
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            domain TEXT NOT NULL,
            path TEXT NOT NULL DEFAULT '/',
            expires BIGINT,
            http_only BOOLEAN NOT NULL DEFAULT FALSE,
            secure BOOLEAN NOT NULL DEFAULT FALSE,
            created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint,
            updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint,
            UNIQUE(platform, name, domain)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cookies_platform ON cookies(platform)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cookies_domain ON cookies(domain)")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS credentials (
            platform TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            password_encrypted TEXT NOT NULL,
            twofa_method TEXT,
            created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint,
            updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::bigint
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
