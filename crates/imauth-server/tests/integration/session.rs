use imauth_proto::generated::v1::{
    session_service_client::SessionServiceClient, Cookie, Empty, ExportRequest, GetCookiesRequest,
    Platform, UpdateCookiesRequest, ValidateRequest,
};

use super::support::{start_test_server, test_context, with_key};

fn session_cookie(expires: i64) -> Cookie {
    Cookie {
        name: "sessionid".to_string(),
        value: "abc123".to_string(),
        domain: ".instagram.com".to_string(),
        path: "/".to_string(),
        expires,
        http_only: true,
        secure: true,
    }
}

#[tokio::test]
async fn cookie_crud_and_export_round_trip_over_grpc() {
    // Given: a real session service backed by fresh SQLite storage.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: a cookie is saved, read, and exported over gRPC.
    let updated = client
        .update_cookies(with_key(UpdateCookiesRequest {
            platform: Platform::Instagram as i32,
            cookies: vec![session_cookie(0)],
        }))
        .await
        .unwrap()
        .into_inner();
    let loaded = client
        .get_cookies(with_key(GetCookiesRequest {
            platform: Platform::Instagram as i32,
            domains: Vec::new(),
        }))
        .await
        .unwrap()
        .into_inner();
    let exported = client
        .export_netscape(with_key(ExportRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: plaintext cookie values cross the wire without leaking ciphertext.
    assert_eq!(updated.cookies.len(), 1);
    assert_eq!(loaded.cookies.len(), 1);
    assert_eq!(loaded.cookies[0].name, "sessionid");
    assert_eq!(loaded.cookies[0].value, "abc123");
    assert!(exported.content.contains("sessionid"));
    assert!(exported.content.contains("abc123"));
    assert!(!exported.content.contains("enc:v1:"));
}

#[tokio::test]
async fn validate_session_reports_invalid_without_session_cookie() {
    // Given: a fresh session service with no cookie state.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: Instagram session validity is requested.
    let result = client
        .validate_session(with_key(ValidateRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: the wire response identifies the absent session cookie.
    assert!(!result.valid);
    assert_eq!(result.expires_at, 0);
    assert_eq!(result.session_cookie_name, "sessionid");
}

#[tokio::test]
async fn validate_session_reports_valid_for_non_expiring_session_cookie() {
    // Given: a non-expiring session cookie persisted in fresh storage.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();
    client
        .update_cookies(with_key(UpdateCookiesRequest {
            platform: Platform::Instagram as i32,
            cookies: vec![session_cookie(0)],
        }))
        .await
        .unwrap();

    // When: Instagram session validity is requested.
    let result = client
        .validate_session(with_key(ValidateRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: the stable non-expiring fixture is reported as valid.
    assert!(result.valid);
    assert_eq!(result.expires_at, 0);
}

#[tokio::test]
async fn connection_status_tracks_platform_cookie_state() {
    // Given: a fresh session service with no platform connections.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();
    let initial = client
        .get_connection_status(with_key(Empty {}))
        .await
        .unwrap()
        .into_inner();

    // When: an Instagram session cookie is persisted.
    client
        .update_cookies(with_key(UpdateCookiesRequest {
            platform: Platform::Instagram as i32,
            cookies: vec![session_cookie(0)],
        }))
        .await
        .unwrap();

    // Then: only Instagram transitions to connected.
    let updated = client
        .get_connection_status(with_key(Empty {}))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(initial.platforms.get("instagram"), Some(&false));
    assert_eq!(initial.platforms.get("threads"), Some(&false));
    assert_eq!(updated.platforms.get("instagram"), Some(&true));
    assert_eq!(updated.platforms.get("threads"), Some(&false));
}

#[tokio::test]
async fn validate_session_rejects_unknown_platform() {
    // Given: a fresh authenticated session service.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: the unset platform value is sent.
    let error = client
        .validate_session(with_key(ValidateRequest { platform: 0 }))
        .await
        .unwrap_err();

    // Then: the wire response rejects malformed platform input.
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}
