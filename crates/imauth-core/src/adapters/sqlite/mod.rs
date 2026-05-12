pub mod cookie_repo;
pub mod credential_repo;
pub mod refresh_repo;
pub mod session_repo;

pub use cookie_repo::SqliteCookieRepository;
pub use credential_repo::SqliteCredentialRepository;
#[allow(unused_imports)]
pub use refresh_repo::SqliteRefreshTokenRepository;
#[allow(unused_imports)]
pub use session_repo::SqliteSessionRepository;

use crate::config::Config;
use crate::ImauthError;
use crate::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::time::Duration;

pub async fn init_pool(config: &Config) -> Result<SqlitePool> {
    let db_path = config.db_path();
    let db_dir = db_path.parent().ok_or_else(|| {
        ImauthError::Config(format!(
            "db_path {} has no parent directory",
            db_path.display()
        ))
    })?;
    std::fs::create_dir_all(db_dir)?;

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    Ok(SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?)
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            platform TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'idle',
            message TEXT,
            requires_input INTEGER NOT NULL DEFAULT 0,
            input_type TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch())
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
            expires_at INTEGER,
            last_refreshed_at INTEGER,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch())
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cookies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            platform TEXT NOT NULL,
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            domain TEXT NOT NULL,
            path TEXT NOT NULL DEFAULT '/',
            expires INTEGER,
            http_only INTEGER NOT NULL DEFAULT 0,
            secure INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
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
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at INTEGER NOT NULL DEFAULT (unixepoch())
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
