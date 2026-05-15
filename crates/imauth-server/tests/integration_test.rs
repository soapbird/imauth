#![allow(clippy::result_large_err)]

use std::sync::Arc;

use imauth_core::{
    adapters::aes_gcm::AesGcmEncryptionService,
    adapters::sqlite::{
        init_pool, run_migrations, SqliteCookieRepository, SqliteCredentialRepository,
        SqliteSessionRepository,
    },
    application::{
        cookies::{
            ExportNetscapeUseCase, GetConnectionStatusUseCase, GetCookiesUseCase,
            UpdateCookiesUseCase, ValidateSessionUseCase,
        },
        credentials::{DeleteCredentialUseCase, GetCredentialUseCase, SaveCredentialUseCase},
        login::LoginUseCase,
        status::{CancelSessionUseCase, GetStatusUseCase},
        submit_2fa::Submit2FaUseCase,
        AppContainer,
    },
    config::Config,
    domain::{session::Cookie, Platform},
    ports::browser::{BrowserSession, BrowserSessionFactory, PageDriver, PlatformDriver},
    ports::snapshot::SnapshotSink,
};
use imauth_proto::generated::v1::{
    credential_service_client::CredentialServiceClient,
    credential_service_server::CredentialServiceServer,
    session_service_client::SessionServiceClient, session_service_server::SessionServiceServer,
    Cookie as ProtoCookie, DeleteCredentialRequest, ExportRequest, GetCookiesRequest,
    GetCredentialRequest, Platform as ProtoPlatform, SaveCredentialRequest, UpdateCookiesRequest,
};
use imauth_server::grpc::{CredentialGrpcService, SessionGrpcService};

// ---------------------------------------------------------------------------
// Test-only stubs
// ---------------------------------------------------------------------------

struct DummyBrowserFactory;

#[async_trait::async_trait]
impl BrowserSessionFactory for DummyBrowserFactory {
    async fn acquire(&self) -> imauth_core::Result<Box<dyn BrowserSession>> {
        unimplemented!("dummy browser factory")
    }
}

#[allow(dead_code)]
struct DummyBrowserSession;

#[async_trait::async_trait]
impl BrowserSession for DummyBrowserSession {
    async fn new_page(&self) -> imauth_core::Result<Box<dyn PageDriver>> {
        unimplemented!("dummy browser session")
    }
    async fn existing_pages(&self) -> imauth_core::Result<Vec<Box<dyn PageDriver>>> {
        unimplemented!("dummy browser session")
    }
}

struct DummyPlatformDriver;

#[async_trait::async_trait]
impl PlatformDriver for DummyPlatformDriver {
    async fn login<'a>(
        &'a self,
        _page: &'a dyn PageDriver,
        _platform: Platform,
        _username: &'a str,
        _password: &'a str,
        _session: &'a mut imauth_core::domain::session::Session,
        _snapshot: &'a dyn SnapshotSink,
    ) -> imauth_core::Result<Vec<Cookie>> {
        unimplemented!("dummy platform driver")
    }

    async fn submit_2fa<'a>(
        &'a self,
        _page: &'a dyn PageDriver,
        _platform: Platform,
        _code: &'a str,
        _session: &'a mut imauth_core::domain::session::Session,
        _snapshot: &'a dyn SnapshotSink,
    ) -> imauth_core::Result<Vec<Cookie>> {
        unimplemented!("dummy platform driver")
    }
}

struct DummySnapshotSink;

#[async_trait::async_trait]
impl SnapshotSink for DummySnapshotSink {
    async fn capture<'a>(
        &'a self,
        _session_id: &'a str,
        _label: &'a str,
        _html: &'a str,
        _png: Option<&'a [u8]>,
    ) {
    }
}

// ---------------------------------------------------------------------------
// Helper: build a test AppContainer backed by an in-memory SQLite database
// ---------------------------------------------------------------------------

async fn test_container() -> AppContainer {
    let mut config = Config::default();
    let temp_dir = std::env::temp_dir().join(format!("imauth_test_{}", uuid::Uuid::new_v4()));
    config.storage.data_dir = temp_dir;
    config.security.encryption_key =
        Some("pZN6lLjwDGIpj/BUWeTFnsB7GUp9bSuwnUcS3gYkQ2A=".to_string());

    let pool = init_pool(&config).await.unwrap();
    run_migrations(&pool).await.unwrap();

    let encryption: Arc<dyn imauth_core::ports::encryption::EncryptionService> =
        Arc::new(AesGcmEncryptionService::from_config(&config).unwrap());

    let sessions: Arc<dyn imauth_core::ports::repository::SessionRepository> =
        Arc::new(SqliteSessionRepository::new(pool.clone()));
    let cookies: Arc<dyn imauth_core::ports::repository::CookieRepository> =
        Arc::new(SqliteCookieRepository::new(pool.clone()));
    let credentials: Arc<dyn imauth_core::ports::repository::CredentialRepository> = Arc::new(
        SqliteCredentialRepository::new(pool.clone(), encryption.clone()),
    );

    let browser: Arc<dyn BrowserSessionFactory> = Arc::new(DummyBrowserFactory);
    let mut drivers = std::collections::HashMap::new();
    drivers.insert(
        Platform::Instagram,
        Arc::new(DummyPlatformDriver) as Arc<dyn PlatformDriver>,
    );
    let snapshot: Arc<dyn SnapshotSink> = Arc::new(DummySnapshotSink);

    AppContainer {
        config,
        login: Arc::new(LoginUseCase::new(
            sessions.clone(),
            cookies.clone(),
            browser.clone(),
            drivers.clone(),
            snapshot.clone(),
        )),
        submit_2fa: Arc::new(Submit2FaUseCase::new(
            sessions.clone(),
            cookies.clone(),
            browser.clone(),
            drivers.clone(),
            snapshot.clone(),
        )),
        get_cookies: Arc::new(GetCookiesUseCase::new(cookies.clone())),
        update_cookies: Arc::new(UpdateCookiesUseCase::new(cookies.clone())),
        export_netscape: Arc::new(ExportNetscapeUseCase::new(cookies.clone())),
        validate_session: Arc::new(ValidateSessionUseCase::new(cookies.clone())),
        get_connection_status: Arc::new(GetConnectionStatusUseCase::new(cookies.clone())),
        save_credential: Arc::new(SaveCredentialUseCase::new(credentials.clone())),
        get_credential: Arc::new(GetCredentialUseCase::new(credentials.clone())),
        delete_credential: Arc::new(DeleteCredentialUseCase::new(credentials.clone())),
        get_status: Arc::new(GetStatusUseCase::new(sessions.clone())),
        cancel_session: Arc::new(CancelSessionUseCase::new(sessions.clone())),
    }
}

async fn start_test_server(container: Arc<AppContainer>, api_key: Option<String>) -> String {
    let session = SessionGrpcService::new(container.clone());
    let credential = CredentialGrpcService::new(container.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let key = api_key.map(std::sync::Arc::new);
        tonic::transport::Server::builder()
            .layer(tonic::service::interceptor(
                move |req: tonic::Request<()>| {
                    if let Some(ref key) = key {
                        let provided = req
                            .metadata()
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.strip_prefix("Bearer "))
                            .or_else(|| {
                                req.metadata()
                                    .get("x-api-key")
                                    .and_then(|v| v.to_str().ok())
                            });

                        match provided {
                            Some(k) if k == **key => Ok(req),
                            _ => Err(tonic::Status::unauthenticated("Invalid or missing API key")),
                        }
                    } else {
                        Ok(req)
                    }
                },
            ))
            .add_service(SessionServiceServer::new(session))
            .add_service(CredentialServiceServer::new(credential))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    format!("http://{}", addr)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

fn with_key<T>(msg: T) -> tonic::Request<T> {
    let mut req = tonic::Request::new(msg);
    req.metadata_mut().insert(
        "authorization",
        "Bearer test-api-key".parse().expect("valid ascii"),
    );
    req
}

#[tokio::test]
async fn test_credential_crud() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;

    let mut client = CredentialServiceClient::connect(addr).await.unwrap();

    // Save
    let resp = client
        .save(with_key(SaveCredentialRequest {
            platform: ProtoPlatform::Instagram as i32,
            username: "testuser".into(),
            password: "testpass".into(),
            twofa_method: "sms".into(),
        }))
        .await
        .unwrap();
    assert!(resp.into_inner().success);

    // Get
    let resp = client
        .get(with_key(GetCredentialRequest {
            platform: ProtoPlatform::Instagram as i32,
        }))
        .await
        .unwrap();
    let info = resp.into_inner();
    assert_eq!(info.username, "testuser");
    assert!(info.has_password);

    // Delete
    let resp = client
        .delete(with_key(DeleteCredentialRequest {
            platform: ProtoPlatform::Instagram as i32,
        }))
        .await
        .unwrap();
    assert!(resp.into_inner().success);
}

#[tokio::test]
async fn test_cookie_crud() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;

    let mut client = SessionServiceClient::connect(addr).await.unwrap();

    // Update cookies
    let resp = client
        .update_cookies(with_key(UpdateCookiesRequest {
            platform: ProtoPlatform::Instagram as i32,
            cookies: vec![ProtoCookie {
                name: "sessionid".into(),
                value: "abc123".into(),
                domain: ".instagram.com".into(),
                path: "/".into(),
                expires: 0,
                http_only: true,
                secure: true,
            }],
        }))
        .await
        .unwrap();
    let cookies = resp.into_inner().cookies;
    assert_eq!(cookies.len(), 1);

    // Get cookies
    let resp = client
        .get_cookies(with_key(GetCookiesRequest {
            platform: ProtoPlatform::Instagram as i32,
            domains: vec![],
        }))
        .await
        .unwrap();
    let cookies = resp.into_inner().cookies;
    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].name, "sessionid");

    // Export netscape
    let resp = client
        .export_netscape(with_key(ExportRequest {
            platform: ProtoPlatform::Instagram as i32,
        }))
        .await
        .unwrap();
    let content = resp.into_inner().content;
    assert!(content.contains("sessionid"));
}

#[tokio::test]
async fn test_auth_rejected_without_api_key() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;

    let mut client = CredentialServiceClient::connect(addr).await.unwrap();

    let resp = client
        .save(tonic::Request::new(SaveCredentialRequest {
            platform: ProtoPlatform::Instagram as i32,
            username: "testuser".into(),
            password: "testpass".into(),
            twofa_method: "sms".into(),
        }))
        .await;

    assert!(resp.is_err());
    let err = resp.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn test_auth_rejected_with_wrong_api_key() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;

    let mut client = CredentialServiceClient::connect(addr).await.unwrap();

    let mut req = tonic::Request::new(SaveCredentialRequest {
        platform: ProtoPlatform::Instagram as i32,
        username: "testuser".into(),
        password: "testpass".into(),
        twofa_method: "sms".into(),
    });
    req.metadata_mut().insert(
        "authorization",
        "Bearer wrong-key".parse().expect("valid ascii"),
    );

    let resp = client.save(req).await;

    assert!(resp.is_err());
    let err = resp.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
}
