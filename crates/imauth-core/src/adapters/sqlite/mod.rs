pub mod cookie_repo;
pub mod credential_repo;
pub mod session_repo;

pub use cookie_repo::SqliteCookieRepository;
pub use credential_repo::SqliteCredentialRepository;
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

    // Tighten dir + db file perms: SQLite stores AES-encrypted credentials and
    // cookies, but the AES key is in env/config — anyone who can read these
    // files plus the key plaintext recovers credentials. chmod 0700 on dir
    // and 0600 on db/-wal/-shm files keeps them off other host UIDs when
    // /data is bind-mounted.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(db_dir, std::fs::Permissions::from_mode(0o700));
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for suffix in ["", "-wal", "-shm"] {
            let p = if suffix.is_empty() {
                db_path.clone()
            } else {
                let mut s = db_path.clone().into_os_string();
                s.push(suffix);
                std::path::PathBuf::from(s)
            };
            if p.exists() {
                let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
            }
        }
    }

    Ok(pool)
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
