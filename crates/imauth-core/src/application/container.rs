use crate::adapters::aes_gcm::AesGcmEncryptionService;
use crate::adapters::chromiumoxide::PooledBrowserFactory;
use crate::adapters::sqlite::{
    self, SqliteCookieRepository, SqliteCredentialRepository, SqliteSessionRepository,
};
use crate::application::cookies::{
    ExportNetscapeUseCase, GetConnectionStatusUseCase, GetCookiesUseCase, UpdateCookiesUseCase,
    ValidateSessionUseCase,
};
use crate::application::credentials::{
    DeleteCredentialUseCase, GetCredentialUseCase, SaveCredentialUseCase,
};
use crate::application::login::LoginUseCase;
use crate::application::status::{CancelSessionUseCase, GetStatusUseCase};
use crate::config::Config;
use crate::ports::browser::BrowserSessionFactory;
use crate::ports::encryption::EncryptionService;
use crate::ports::repository::{CookieRepository, CredentialRepository, SessionRepository};
use crate::Result;
use std::sync::Arc;
use std::time::Duration;

/// Composition root. Wires concrete adapters into use cases. `imauth-server` and
/// future delivery layers depend on this and nothing else from `imauth-core`'s
/// public surface beyond domain types.
pub struct AppContainer {
    pub config: Config,
    pub login: Arc<LoginUseCase>,
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
        let encryption: Arc<dyn EncryptionService> =
            Arc::new(AesGcmEncryptionService::from_config(&config)?);

        let pool = sqlite::init_pool(&config).await?;
        sqlite::run_migrations(&pool).await?;
        let sessions: Arc<dyn SessionRepository> =
            Arc::new(SqliteSessionRepository::new(pool.clone()));
        let cookies: Arc<dyn CookieRepository> = Arc::new(SqliteCookieRepository::new(
            pool.clone(),
            encryption.clone(),
        ));
        let credentials: Arc<dyn CredentialRepository> = Arc::new(SqliteCredentialRepository::new(
            pool.clone(),
            encryption.clone(),
        ));

        let cdp_urls = config.cdp_urls();
        let viewer_urls = config.browser_viewer_urls();
        let login_timeout = Duration::from_secs(config.login_timeout_secs());
        let acquire_timeout = Duration::from_secs(config.browser.acquire_timeout_secs);
        let connect_timeout = Duration::from_secs(config.browser.connect_timeout_secs);
        let page_timeout = Duration::from_secs(config.page_timeout_secs());
        let preparation_timeout = acquire_timeout
            + connect_timeout.saturating_mul(cdp_urls.len().try_into().unwrap_or(u32::MAX))
            + page_timeout.saturating_mul(2);
        let browser: Arc<dyn BrowserSessionFactory> = Arc::new(PooledBrowserFactory::new(
            cdp_urls,
            &viewer_urls,
            acquire_timeout,
            connect_timeout,
        ));

        let login = Arc::new(
            LoginUseCase::new(
                sessions.clone(),
                cookies.clone(),
                browser.clone(),
                login_timeout,
            )
            .with_browser_timeouts(preparation_timeout, page_timeout),
        );
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
