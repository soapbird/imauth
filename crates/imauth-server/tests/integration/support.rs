use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use imauth_core::{
    adapters::aes_gcm::AesGcmEncryptionService,
    adapters::sqlite::{
        run_migrations, SqliteCookieRepository, SqliteCredentialRepository, SqliteSessionRepository,
    },
    application::{
        cookies::{
            ExportNetscapeUseCase, GetConnectionStatusUseCase, GetCookiesUseCase,
            UpdateCookiesUseCase, ValidateSessionUseCase,
        },
        credentials::{DeleteCredentialUseCase, GetCredentialUseCase, SaveCredentialUseCase},
        login::LoginUseCase,
        status::{CancelSessionUseCase, GetStatusUseCase},
        AppContainer,
    },
    config::Config,
    domain::session::Cookie,
    ports::browser::{BrowserSession, BrowserSessionFactory, PageDriver},
    ImauthError,
};
use imauth_proto::generated::v1::{
    auth_service_server::AuthServiceServer, credential_service_server::CredentialServiceServer,
    session_service_server::SessionServiceServer,
};
use imauth_server::grpc::{AuthGrpcService, CredentialGrpcService, SessionGrpcService};
use sqlx::SqlitePool;

const KEY: &str = "pZN6lLjwDGIpj/BUWeTFnsB7GUp9bSuwnUcS3gYkQ2A=";

pub struct TestContext {
    pub container: Arc<AppContainer>,
    pub pool: SqlitePool,
}

pub struct TestServer {
    pub endpoint: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub struct FakeBrowserObservations {
    acquired: AtomicUsize,
    navigated: AtomicUsize,
    closed: AtomicUsize,
}

impl FakeBrowserObservations {
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.acquired.load(Ordering::SeqCst),
            self.navigated.load(Ordering::SeqCst),
            self.closed.load(Ordering::SeqCst),
        )
    }
}

pub struct DeterministicBrowserFactory {
    observations: Arc<FakeBrowserObservations>,
}

impl DeterministicBrowserFactory {
    pub fn new(observations: Arc<FakeBrowserObservations>) -> Self {
        Self { observations }
    }
}

#[async_trait::async_trait]
impl BrowserSessionFactory for DeterministicBrowserFactory {
    async fn acquire(&self) -> imauth_core::Result<Box<dyn BrowserSession>> {
        self.observations.acquired.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(DeterministicBrowserSession {
            observations: self.observations.clone(),
        }))
    }

    fn viewer_url(&self) -> Option<String> {
        Some("http://viewer.test/session".to_string())
    }
}

struct DeterministicBrowserSession {
    observations: Arc<FakeBrowserObservations>,
}

#[async_trait::async_trait]
impl BrowserSession for DeterministicBrowserSession {
    async fn new_page(&self) -> imauth_core::Result<Box<dyn PageDriver>> {
        Ok(Box::new(DeterministicPageDriver {
            observations: self.observations.clone(),
        }))
    }

    async fn existing_pages(&self) -> imauth_core::Result<Vec<Box<dyn PageDriver>>> {
        Ok(Vec::new())
    }

    fn viewer_url(&self) -> String {
        "http://viewer.test/session".to_string()
    }
}

struct DeterministicPageDriver {
    observations: Arc<FakeBrowserObservations>,
}

#[async_trait::async_trait]
impl PageDriver for DeterministicPageDriver {
    async fn navigate(&self, _url: &str, _timeout_secs: u64) -> imauth_core::Result<()> {
        self.observations.navigated.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn get_cookies(&self) -> imauth_core::Result<Vec<Cookie>> {
        Ok(Vec::new())
    }

    async fn screenshot(&self) -> imauth_core::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn content_html(&self) -> imauth_core::Result<String> {
        Ok(String::new())
    }

    async fn close(&self) -> imauth_core::Result<()> {
        self.observations.closed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct RejectingBrowserFactory;

#[async_trait::async_trait]
impl BrowserSessionFactory for RejectingBrowserFactory {
    async fn acquire(&self) -> imauth_core::Result<Box<dyn BrowserSession>> {
        Err(ImauthError::Browser(
            "browser use was not expected".to_string(),
        ))
    }

    fn viewer_url(&self) -> Option<String> {
        None
    }
}

pub async fn test_context() -> TestContext {
    test_context_with_browser(Arc::new(RejectingBrowserFactory), Duration::from_secs(300)).await
}

pub async fn test_context_with_browser(
    browser: Arc<dyn BrowserSessionFactory>,
    login_timeout: Duration,
) -> TestContext {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut config = Config::default();
    config.security.encryption_key = Some(KEY.to_string());
    let encryption: Arc<dyn imauth_core::ports::encryption::EncryptionService> =
        Arc::new(AesGcmEncryptionService::from_config(&config).unwrap());
    let sessions: Arc<dyn imauth_core::ports::repository::SessionRepository> =
        Arc::new(SqliteSessionRepository::new(pool.clone()));
    let cookies: Arc<dyn imauth_core::ports::repository::CookieRepository> = Arc::new(
        SqliteCookieRepository::new(pool.clone(), encryption.clone()),
    );
    let credentials: Arc<dyn imauth_core::ports::repository::CredentialRepository> =
        Arc::new(SqliteCredentialRepository::new(pool.clone(), encryption));
    let container = AppContainer {
        config,
        login: Arc::new(LoginUseCase::new(
            sessions.clone(),
            cookies.clone(),
            browser,
            login_timeout,
        )),
        get_cookies: Arc::new(GetCookiesUseCase::new(cookies.clone())),
        update_cookies: Arc::new(UpdateCookiesUseCase::new(cookies.clone())),
        export_netscape: Arc::new(ExportNetscapeUseCase::new(cookies.clone())),
        validate_session: Arc::new(ValidateSessionUseCase::new(cookies.clone())),
        get_connection_status: Arc::new(GetConnectionStatusUseCase::new(cookies)),
        save_credential: Arc::new(SaveCredentialUseCase::new(credentials.clone())),
        get_credential: Arc::new(GetCredentialUseCase::new(credentials.clone())),
        delete_credential: Arc::new(DeleteCredentialUseCase::new(credentials)),
        get_status: Arc::new(GetStatusUseCase::new(sessions.clone())),
        cancel_session: Arc::new(CancelSessionUseCase::new(sessions)),
    };
    TestContext {
        container: Arc::new(container),
        pool,
    }
}

pub async fn start_test_server(context: &TestContext, api_key: Option<String>) -> TestServer {
    let auth = AuthGrpcService::new(context.container.clone());
    let session = SessionGrpcService::new(context.container.clone());
    let credential = CredentialGrpcService::new(context.container.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let key = imauth_server::auth::normalize_api_key(api_key).map(Arc::new);
    let auth_interceptor = imauth_server::auth::auth_interceptor(key.clone());
    let session_interceptor = imauth_server::auth::auth_interceptor(key.clone());
    let credential_interceptor = imauth_server::auth::auth_interceptor(key);
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<AuthServiceServer<AuthGrpcService>>()
        .await;
    health_reporter
        .set_serving::<SessionServiceServer<SessionGrpcService>>()
        .await;
    health_reporter
        .set_serving::<CredentialServiceServer<CredentialGrpcService>>()
        .await;
    let task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(AuthServiceServer::with_interceptor(auth, auth_interceptor))
            .add_service(SessionServiceServer::with_interceptor(
                session,
                session_interceptor,
            ))
            .add_service(CredentialServiceServer::with_interceptor(
                credential,
                credential_interceptor,
            ))
            .add_service(health_service)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    TestServer {
        endpoint: format!("http://{addr}"),
        task,
    }
}

pub fn with_key<T>(message: T) -> tonic::Request<T> {
    let mut request = tonic::Request::new(message);
    request.metadata_mut().insert(
        "authorization",
        "Bearer test-api-key".parse().expect("valid ASCII"),
    );
    request
}
