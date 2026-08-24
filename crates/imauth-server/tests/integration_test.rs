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
        AppContainer,
    },
    config::Config,
    ports::browser::{BrowserSessionFactory, BrowserSession, PageDriver},
};
use imauth_proto::generated::v1::{
    credential_service_client::CredentialServiceClient,
    credential_service_server::CredentialServiceServer, session_service_client::SessionServiceClient,
    session_service_server::SessionServiceServer, Cookie as ProtoCookie, DeleteCredentialRequest,
    Empty, ExportRequest, GetCookiesRequest, GetCredentialRequest, Platform as ProtoPlatform,
    SaveCredentialRequest, UpdateCookiesRequest, ValidateRequest,
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

    fn viewer_url(&self) -> Option<String> {
        None
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
    fn viewer_url(&self) -> String {
        String::new()
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
    let cookies: Arc<dyn imauth_core::ports::repository::CookieRepository> = Arc::new(
        SqliteCookieRepository::new(pool.clone(), encryption.clone()),
    );
    let credentials: Arc<dyn imauth_core::ports::repository::CredentialRepository> = Arc::new(
        SqliteCredentialRepository::new(pool.clone(), encryption.clone()),
    );

    let browser: Arc<dyn BrowserSessionFactory> = Arc::new(DummyBrowserFactory);

    AppContainer {
        config,
        login: Arc::new(LoginUseCase::new(
            sessions.clone(),
            cookies.clone(),
            browser.clone(),
            std::time::Duration::from_secs(300),
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

    // Use the production interceptor + key normalizer (imauth_server::auth)
    // so the test exercises the same constant-time comparison and whitespace
    // handling as the binary. Bypassing them in the test was hiding regressions.
    let key = imauth_server::auth::normalize_api_key(api_key).map(std::sync::Arc::new);
    let session_interceptor = imauth_server::auth::auth_interceptor(key.clone());
    let credential_interceptor = imauth_server::auth::auth_interceptor(key);

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<SessionServiceServer<SessionGrpcService>>()
        .await;
    health_reporter
        .set_serving::<CredentialServiceServer<CredentialGrpcService>>()
        .await;

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(SessionServiceServer::with_interceptor(session, session_interceptor))
            .add_service(CredentialServiceServer::with_interceptor(
                credential,
                credential_interceptor,
            ))
            .add_service(health_service)
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
    assert_eq!(cookies[0].value, "abc123");

    // Export netscape
    let resp = client
        .export_netscape(with_key(ExportRequest {
            platform: ProtoPlatform::Instagram as i32,
        }))
        .await
        .unwrap();
    let content = resp.into_inner().content;
    assert!(content.contains("sessionid"));
    assert!(content.contains("abc123"));
    assert!(!content.contains("enc:v1:"));
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
async fn test_validate_session_reports_invalid_when_no_session_cookie() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(addr).await.unwrap();

    let resp = client
        .validate_session(with_key(ValidateRequest {
            platform: ProtoPlatform::Instagram as i32,
        }))
        .await
        .unwrap();
    let result = resp.into_inner();
    assert!(!result.valid);
    assert_eq!(result.expires_at, 0);
    assert_eq!(result.session_cookie_name, "sessionid");
}

#[tokio::test]
async fn test_validate_session_reports_valid_after_session_cookie_persisted() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(addr).await.unwrap();

    client
        .update_cookies(with_key(UpdateCookiesRequest {
            platform: ProtoPlatform::Instagram as i32,
            cookies: vec![ProtoCookie {
                name: "sessionid".into(),
                value: "abc123".into(),
                domain: ".instagram.com".into(),
                path: "/".into(),
                expires: 1_700_000_000,
                http_only: true,
                secure: true,
            }],
        }))
        .await
        .unwrap();

    let resp = client
        .validate_session(with_key(ValidateRequest {
            platform: ProtoPlatform::Instagram as i32,
        }))
        .await
        .unwrap();
    let result = resp.into_inner();
    assert!(result.valid);
    assert_eq!(result.expires_at, 1_700_000_000);
}

#[tokio::test]
async fn test_get_connection_status_includes_all_known_platforms() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(addr).await.unwrap();

    let resp = client
        .get_connection_status(with_key(Empty {}))
        .await
        .unwrap();
    let map = resp.into_inner().platforms;
    assert_eq!(map.get("instagram"), Some(&false));
    assert_eq!(map.get("threads"), Some(&false));

    client
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

    let resp = client
        .get_connection_status(with_key(Empty {}))
        .await
        .unwrap();
    let map = resp.into_inner().platforms;
    assert_eq!(map.get("instagram"), Some(&true));
    assert_eq!(map.get("threads"), Some(&false));
}

#[tokio::test]
async fn test_get_credential_returns_not_found_when_missing() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;
    let mut client = CredentialServiceClient::connect(addr).await.unwrap();

    let resp = client
        .get(with_key(GetCredentialRequest {
            platform: ProtoPlatform::Instagram as i32,
        }))
        .await;
    let err = resp.expect_err("expected NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_invalid_platform_returns_invalid_argument() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(addr).await.unwrap();

    let resp = client
        .validate_session(with_key(ValidateRequest {
            platform: 0, // unset / unknown platform
        }))
        .await;
    let err = resp.expect_err("expected InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_auth_accepts_x_api_key_alt_header() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(addr).await.unwrap();

    let mut req = tonic::Request::new(GetCookiesRequest {
        platform: ProtoPlatform::Instagram as i32,
        domains: vec![],
    });
    req.metadata_mut()
        .insert("x-api-key", "test-api-key".parse().expect("valid ascii"));

    // Should succeed: interceptor falls back to x-api-key when Authorization is absent.
    let resp = client.get_cookies(req).await.unwrap();
    assert!(resp.into_inner().cookies.is_empty());
}

#[tokio::test]
async fn test_health_check_returns_serving_without_auth() {
    let container = Arc::new(test_container().await);
    let addr = start_test_server(container, Some("test-api-key".to_string())).await;

    let channel = tonic::transport::Channel::from_shared(addr)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    let resp = client
        .check(tonic_health::pb::HealthCheckRequest {
            service: "".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(
        resp.into_inner().status,
        tonic_health::pb::health_check_response::ServingStatus::Serving as i32
    );
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
