use std::sync::Arc;
use std::time::Duration;

use imauth_proto::generated::v1::{
    auth_service_client::AuthServiceClient, AuthStatus, CancelRequest, LoginRequest, Platform,
    StatusRequest,
};

use super::support::{
    start_test_server, test_context, test_context_with_browser, with_key,
    DeterministicBrowserFactory, FakeBrowserObservations,
};

async fn insert_session(pool: &sqlx::SqlitePool, session_id: &str) {
    sqlx::query(
        "INSERT INTO sessions \
         (id, platform, status, message, requires_input, input_type, created_at, updated_at) \
         VALUES (?1, 'instagram', 'waiting_for_user', 'finish login', 1, 'viewer_url', 1700000000, 1700000123)",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn get_status_returns_existing_session_over_grpc() {
    // Given: a persisted waiting session behind the real AuthService transport.
    let context = test_context().await;
    insert_session(&context.pool, "session-existing").await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: the session status is requested over gRPC.
    let response = client
        .get_status(with_key(StatusRequest {
            session_id: "session-existing".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: the wire response preserves session state and input fields.
    assert_eq!(response.session_id, "session-existing");
    assert_eq!(response.status, AuthStatus::WaitingForUser as i32);
    assert_eq!(response.message, "finish login");
    assert!(response.requires_input);
    assert_eq!(response.input_type, "viewer_url");
}

#[tokio::test]
async fn get_status_returns_not_found_for_missing_session_id() {
    // Given: a fresh AuthService with no session rows.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: an empty missing session ID is requested.
    let error = client
        .get_status(with_key(StatusRequest {
            session_id: String::new(),
        }))
        .await
        .unwrap_err();

    // Then: the actual contract reports NotFound.
    assert_eq!(error.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn cancel_removes_existing_session_over_grpc() {
    // Given: a persisted session behind the real AuthService transport.
    let context = test_context().await;
    insert_session(&context.pool, "session-cancel").await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: the session is cancelled over gRPC.
    let response = client
        .cancel(with_key(CancelRequest {
            session_id: "session-cancel".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: success is returned and the persisted session is removed.
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id = 'session-cancel'")
            .fetch_one(&context.pool)
            .await
            .unwrap();
    assert!(response.success);
    assert_eq!(response.session_id, "session-cancel");
    assert_eq!(remaining, 0);
}

#[tokio::test]
async fn cancel_missing_session_is_idempotent() {
    // Given: a fresh AuthService with no session rows.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: a missing session ID is cancelled.
    let response = client
        .cancel(with_key(CancelRequest {
            session_id: "missing".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: the current delete contract remains idempotently successful.
    assert!(response.success);
    assert_eq!(response.session_id, "missing");
}

#[tokio::test]
async fn login_rejects_unknown_platform_before_browser_use() {
    // Given: a fresh authenticated AuthService.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: Login receives the unknown platform value.
    let error = client
        .login(with_key(LoginRequest { platform: 0 }))
        .await
        .unwrap_err();

    // Then: malformed input is rejected at the wire boundary.
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn login_stream_emits_started_waiting_and_terminal_events() {
    // Given: a deterministic browser boundary and zero-duration login deadline.
    let observations = Arc::new(FakeBrowserObservations::default());
    let browser = Arc::new(DeterministicBrowserFactory::new(observations.clone()));
    let context = test_context_with_browser(browser, Duration::ZERO).await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: Login is streamed through the real AuthService transport.
    let mut stream = client
        .login(with_key(LoginRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .unwrap()
        .into_inner();
    let started = stream.message().await.unwrap().unwrap();
    let waiting = stream.message().await.unwrap().unwrap();
    let terminal = stream.message().await.unwrap().unwrap();

    // Then: lifecycle events and fake-boundary observations are deterministic.
    assert!(!started.session_id.is_empty());
    assert_eq!(started.status, AuthStatus::Idle as i32);
    assert_eq!(waiting.session_id, started.session_id);
    assert_eq!(waiting.status, AuthStatus::WaitingForUser as i32);
    assert_eq!(waiting.viewer_url, "http://viewer.test/session");
    assert_eq!(terminal.session_id, started.session_id);
    assert_eq!(terminal.status, AuthStatus::Failed as i32);
    assert!(stream.message().await.unwrap().is_none());
    assert_eq!(observations.counts(), (1, 1, 1));
}
