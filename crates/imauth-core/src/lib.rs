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
use credential::{CredentialStore, Credential};
use session::cookie_jar::CookieJar;
use session::state::{Session, SessionState};
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
            .map_err(|e| ImauthError::Io(e))?;

        let db_url = format!("sqlite:{}", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .map_err(|e| ImauthError::Database(e.to_string()))?;

        // Run migrations / init tables
        let cookie_jar = CookieJar::new(pool.clone());
        cookie_jar.init().await?;

        let encryption = {
            let key = config.encryption_key.clone().unwrap_or_else(|| {
                let key = credential::encryption::AesGcmEncryption::generate_key();
                tracing::warn!("No encryption key configured; generated a temporary one");
                key
            });
            credential::encryption::AesGcmEncryption::from_key(&key)?
        };
        let credential_store = CredentialStore::new(pool.clone(), encryption);
        credential_store.init().await?;

        let browser_manager = BrowserManager::new(
            config.cdp_url.clone(),
            config.max_pool_size,
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
