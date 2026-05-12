use crate::adapters::aes_gcm::AesGcmEncryptionService;
use crate::adapters::chromiumoxide::{ChromiumOxideBrowserFactory, InstagramPlatformDriver};
use crate::adapters::fs::FsSnapshotSink;
use crate::adapters::sqlite::{self, SqliteCookieRepository, SqliteCredentialRepository, SqliteSessionRepository};
use crate::application::cookies::{
    ExportNetscapeUseCase, GetConnectionStatusUseCase, GetCookiesUseCase, UpdateCookiesUseCase,
    ValidateSessionUseCase,
};
use crate::application::credentials::{
    DeleteCredentialUseCase, GetCredentialUseCase, SaveCredentialUseCase,
};
use crate::Result;
use crate::application::login::LoginUseCase;
use crate::application::status::{CancelSessionUseCase, GetStatusUseCase};
use crate::application::submit_2fa::Submit2FaUseCase;
use crate::config::Config;
use crate::domain::Platform;
use crate::ports::browser::{BrowserSessionFactory, PlatformDriver};
use crate::ports::encryption::EncryptionService;
use crate::ports::repository::{CookieRepository, CredentialRepository, SessionRepository};
use crate::ports::snapshot::SnapshotSink;
use std::collections::HashMap;
use std::sync::Arc;

/// Composition root. Wires concrete adapters into use cases. `imauth-server` and
/// future delivery layers depend on this and nothing else from `imauth-core`'s
/// public surface beyond domain types.
pub struct AppContainer {
    pub config: Config,
    pub login: Arc<LoginUseCase>,
    pub submit_2fa: Arc<Submit2FaUseCase>,
    pub get_cookies: Arc<GetCookiesUseCase>,
    pub update_cookies: Arc<UpdateCookiesUseCase>,
    pub export_netscape: Arc<ExportNetscapeUseCase>,
    pub validate_session: Arc<ValidateSessionUseCase>,
    pub get_connection_status: Arc<GetConnectionStatusUseCase>,
    pub save_credential: Arc<SaveCredentialUseCase>,
    pub get_credential: Arc<GetCredentialUseCase>,
    pub delete_credential: Arc<DeleteCredentialUseCase>,
    pub get_status: Arc<GetStatusUseCase>,
    pub cancel_session: Arc<CancelSessionUseCase>,
}

impl AppContainer {
    pub async fn from_config(config: Config) -> Result<Self> {
        let pool = sqlite::init_pool(&config).await?;
        sqlite::run_migrations(&pool).await?;

        let encryption: Arc<dyn EncryptionService> =
            Arc::new(AesGcmEncryptionService::from_config(&config)?);

        let sessions: Arc<dyn SessionRepository> = Arc::new(SqliteSessionRepository::new(pool.clone()));
        let cookies: Arc<dyn CookieRepository> =
            Arc::new(SqliteCookieRepository::new(pool.clone()));
        let credentials: Arc<dyn CredentialRepository> = Arc::new(SqliteCredentialRepository::new(
            pool.clone(),
            encryption.clone(),
        ));

        let browser: Arc<dyn BrowserSessionFactory> = Arc::new(ChromiumOxideBrowserFactory::new(
            config.cdp_url().to_string(),
            config.max_pool_size(),
        ));

        let mut drivers: HashMap<Platform, Arc<dyn PlatformDriver>> = HashMap::new();
        let instagram_driver = Arc::new(InstagramPlatformDriver::new());
        drivers.insert(Platform::Instagram, instagram_driver.clone());
        drivers.insert(Platform::Threads, instagram_driver);

        let snapshot: Arc<dyn SnapshotSink> = Arc::new(FsSnapshotSink::new(config.snapshot_dir()));

        let login = Arc::new(LoginUseCase::new(
            sessions.clone(),
            cookies.clone(),
            browser.clone(),
            drivers.clone(),
            snapshot.clone(),
        ));
        let submit_2fa = Arc::new(Submit2FaUseCase::new(
            sessions.clone(),
            cookies.clone(),
            browser.clone(),
            drivers.clone(),
            snapshot.clone(),
        ));
        let get_cookies = Arc::new(GetCookiesUseCase::new(cookies.clone()));
        let update_cookies = Arc::new(UpdateCookiesUseCase::new(cookies.clone()));
        let export_netscape = Arc::new(ExportNetscapeUseCase::new(cookies.clone()));
        let validate_session = Arc::new(ValidateSessionUseCase::new(cookies.clone()));
        let get_connection_status = Arc::new(GetConnectionStatusUseCase::new(cookies.clone()));
        let save_credential = Arc::new(SaveCredentialUseCase::new(credentials.clone()));
        let get_credential = Arc::new(GetCredentialUseCase::new(credentials.clone()));
        let delete_credential = Arc::new(DeleteCredentialUseCase::new(credentials.clone()));
        let get_status = Arc::new(GetStatusUseCase::new(sessions.clone()));
        let cancel_session = Arc::new(CancelSessionUseCase::new(sessions.clone()));

        Ok(Self {
            config,
            login,
            submit_2fa,
            get_cookies,
            update_cookies,
            export_netscape,
            validate_session,
            get_connection_status,
            save_credential,
            get_credential,
            delete_credential,
            get_status,
            cancel_session,
        })
    }
}
