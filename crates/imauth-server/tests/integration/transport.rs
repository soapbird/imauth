use imauth_proto::generated::v1::{
    credential_service_client::CredentialServiceClient,
    session_service_client::SessionServiceClient, GetCookiesRequest, Platform,
    SaveCredentialRequest,
};

use super::support::{start_test_server, test_context, with_key};

fn save_request() -> SaveCredentialRequest {
    SaveCredentialRequest {
        platform: Platform::Instagram as i32,
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        twofa_method: "sms".to_string(),
    }
}

#[tokio::test]
async fn api_key_is_required_for_business_services() {
    // Given: a server protected by the production API-key interceptor.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = CredentialServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: a request omits authentication metadata.
    let error = client.save(save_request()).await.unwrap_err();

    // Then: the interceptor rejects it as unauthenticated.
    assert_eq!(error.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn wrong_api_key_is_rejected() {
    // Given: a server protected by a different API key.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = CredentialServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();
    let mut request = tonic::Request::new(save_request());
    request.metadata_mut().insert(
        "authorization",
        "Bearer wrong-key".parse().expect("valid ASCII"),
    );

    // When: the request carries the wrong bearer token.
    let error = client.save(request).await.unwrap_err();

    // Then: the interceptor rejects it as unauthenticated.
    assert_eq!(error.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn x_api_key_header_is_accepted() {
    // Given: a server protected by the production API-key interceptor.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();
    let mut request = tonic::Request::new(GetCookiesRequest {
        platform: Platform::Instagram as i32,
        domains: Vec::new(),
    });
    request
        .metadata_mut()
        .insert("x-api-key", "test-api-key".parse().expect("valid ASCII"));

    // When: the alternate API-key header is used.
    let response = client.get_cookies(request).await.unwrap().into_inner();

    // Then: the authenticated business request succeeds.
    assert!(response.cookies.is_empty());
}

#[tokio::test]
async fn health_check_is_serving_without_authentication() {
    // Given: a protected server with unauthenticated production health setup.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let channel = tonic::transport::Channel::from_shared(server.endpoint.clone())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);

    // When: the aggregate health service is checked without metadata.
    let response = client
        .check(tonic_health::pb::HealthCheckRequest {
            service: String::new(),
        })
        .await
        .unwrap()
        .into_inner();

    // Then: health remains serving outside business-service authentication.
    assert_eq!(
        response.status,
        tonic_health::pb::health_check_response::ServingStatus::Serving as i32
    );
}

#[tokio::test]
async fn bearer_api_key_is_accepted() {
    // Given: a server protected by the production API-key interceptor.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = SessionServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: a request uses the normal bearer helper.
    let response = client
        .get_cookies(with_key(GetCookiesRequest {
            platform: Platform::Instagram as i32,
            domains: Vec::new(),
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: the request reaches the service.
    assert!(response.cookies.is_empty());
}
