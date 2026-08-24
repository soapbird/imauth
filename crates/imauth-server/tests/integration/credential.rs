use imauth_proto::generated::v1::{
    credential_service_client::CredentialServiceClient, DeleteCredentialRequest,
    GetCredentialRequest, Platform, SaveCredentialRequest,
};

use super::support::{start_test_server, test_context, with_key};

#[tokio::test]
async fn credential_crud_round_trips_over_grpc() {
    // Given: a real credential service backed by fresh SQLite storage.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = CredentialServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: a credential is saved, retrieved, and deleted over gRPC.
    let saved = client
        .save(with_key(SaveCredentialRequest {
            platform: Platform::Instagram as i32,
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            twofa_method: "sms".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let credential = client
        .get(with_key(GetCredentialRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .unwrap()
        .into_inner();
    let deleted = client
        .delete(with_key(DeleteCredentialRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then: each wire response reflects the persisted credential lifecycle.
    assert!(saved.success);
    assert_eq!(credential.username, "testuser");
    assert!(credential.has_password);
    assert!(deleted.success);
}

#[tokio::test]
async fn get_credential_returns_not_found_when_missing() {
    // Given: a fresh credential service with no stored credentials.
    let context = test_context().await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = CredentialServiceClient::connect(server.endpoint.clone())
        .await
        .unwrap();

    // When: a missing platform credential is requested.
    let error = client
        .get(with_key(GetCredentialRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .unwrap_err();

    // Then: the service returns the NotFound wire contract.
    assert_eq!(error.code(), tonic::Code::NotFound);
}
