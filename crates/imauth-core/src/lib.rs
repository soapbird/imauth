pub mod browser;
pub mod config;
pub mod credential;
pub mod error;
pub mod platform;
pub mod queue;
pub mod refresh;
pub mod session;

pub use config::Config;
pub use error::{ImauthError, Result};

use browser::BrowserManager;
use credential::CredentialStore;
use session::cookie_jar::CookieJar;
use session::state::Session;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ImauthCore {
    pub config: Config,
    pub db_pool: sqlx::SqlitePool,
    pub cookie_jar: CookieJar,
    pub credential_store: CredentialStore,
    pub browser_manager: BrowserManager,
    pub sessions: Arc<RwLock<std::collections::HashMap<String, Session>>>,
}

impl ImauthCore {
    pub async fn new(config: Config) -> Result<Self> {
        let db_path = config.db_path();
        std::fs::create_dir_all(db_path.parent().unwrap())
            .map_err(ImauthError::Io)?;

        let db_url = format!("sqlite:{}", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .map_err(|e| ImauthError::Database(e.to_string()))?;

        // Init tables
        Self::init_db(&pool).await?;

        let cookie_jar = CookieJar::new(pool.clone());
        cookie_jar.init().await?;

        let encryption = {
            let key = config.encryption_key().map(|s| s.to_string()).unwrap_or_else(|| {
                let key = credential::encryption::AesGcmEncryption::generate_key();
                tracing::warn!("No encryption key configured; generated a temporary one");
                key
            });
            credential::encryption::AesGcmEncryption::from_key(&key)?
        };
        let credential_store = CredentialStore::new(pool.clone(), encryption);
        credential_store.init().await?;

        let browser_manager = BrowserManager::new(
            config.cdp_url().to_string(),
            config.max_pool_size(),
        );

        Ok(Self {
            config,
            db_pool: pool,
            cookie_jar,
            credential_store,
            browser_manager,
            sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    async fn init_db(pool: &sqlx::SqlitePool) -> Result<()> {
        // sessions table for persistence across restarts
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
        .await
        .map_err(|e| ImauthError::Database(e.to_string()))?;

        // refresh_tokens table
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
        .await
        .map_err(|e| ImauthError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn create_session(
        &self,
        platform: String,
    ) -> Result<Session> {
        let id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(id, platform);
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<Session>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    pub async fn update_session(
        &self,
        session: &Session,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    pub async fn delete_session(
        &self,
        session_id: &str,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    #[test]
    fn session_proto_uses_correct_korean_export_comment() {
        let proto_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../proto/imauth/v1/session.proto");
        let proto = fs::read_to_string(&proto_path)
            .expect("session.proto should be readable");

        assert!(
            proto.contains("Netscape 포맷으로 쿠키 내보내기"),
            "expected corrected Korean export comment in {}",
            proto_path.display()
        );
        assert!(
            !proto.contains("Netscape 포맷으로 쿠키 낳보기"),
            "expected old Korean typo to be removed from {}",
            proto_path.display()
        );
    }
}
