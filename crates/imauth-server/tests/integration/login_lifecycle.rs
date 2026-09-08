use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use imauth_core::ports::browser::{BrowserSession, BrowserSessionFactory};
use imauth_core::Result as AppResult;
use imauth_proto::generated::v1::{
    auth_service_client::AuthServiceClient, AuthStatus, CancelRequest, LoginRequest, Platform,
    StatusRequest,
};
use tokio::time::{timeout, Duration as TokioDuration};

use super::support::{start_test_server, test_context_with_browser, with_key};

struct PendingBrowserFactory {
    acquire_count: Arc<AtomicUsize>,
}

#[async_trait]
impl BrowserSessionFactory for PendingBrowserFactory {
    async fn acquire(&self) -> AppResult<Box<dyn BrowserSession>> {
        self.acquire_count.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
    }

    fn viewer_url(&self) -> Option<String> {
        None
    }
}

async fn wait_for_acquires(acquire_count: &AtomicUsize, expected: usize) {
    timeout(Duration::from_secs(1), async {
        while acquire_count.load(Ordering::SeqCst) < expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("browser acquire did not become pending in time");
}

#[tokio::test]
async fn login_transport_enforces_capacity_and_releases_dropped_stream() {
    // Given: the default one-CDP plus eight-pending-login admission budget.
    let acquire_count = Arc::new(AtomicUsize::new(0));
    let browser = Arc::new(PendingBrowserFactory {
        acquire_count: acquire_count.clone(),
    });
    let context = test_context_with_browser(browser, Duration::from_secs(300)).await;
    let capacity = context.container.config.cdp_urls().len()
        + context.container.config.browser.max_pending_logins;
    assert_eq!(capacity, 9);

    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut clients = Vec::with_capacity(capacity);
    let mut streams = Vec::with_capacity(capacity);

    // When: nine real Login RPC streams are held while browser acquisition is pending.
    for _ in 0..capacity {
        let mut client = AuthServiceClient::connect(server.endpoint.clone())
            .await
            .expect("connect login client");
        let mut stream = client
            .login(with_key(LoginRequest {
                platform: Platform::Instagram as i32,
            }))
            .await
            .expect("login stream admitted")
            .into_inner();
        let started = timeout(Duration::from_secs(1), stream.message())
            .await
            .expect("started event deadline")
            .expect("started event transport")
            .expect("started event");
        assert_eq!(started.status, AuthStatus::Idle as i32);
        clients.push(client);
        streams.push(stream);
    }
    wait_for_acquires(&acquire_count, capacity).await;

    // Then: the next Login is rejected at the transport boundary.
    let mut rejected_client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .expect("connect rejected client");
    let error = rejected_client
        .login(with_key(LoginRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .expect_err("10th login must be rejected");
    assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    drop(rejected_client);

    // When: one held stream and its client transport are both dropped.
    drop(streams.pop().expect("held stream"));
    drop(clients.pop().expect("held client"));

    // Then: capacity resumes within one second and the replacement stream is admitted.
    let mut resumed_client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .expect("connect resumed client");
    let resumed_response = timeout(TokioDuration::from_secs(1), async {
        loop {
            match resumed_client
                .login(with_key(LoginRequest {
                    platform: Platform::Instagram as i32,
                }))
                .await
            {
                Ok(response) => break Ok(response),
                Err(error) if error.code() == tonic::Code::ResourceExhausted => {
                    tokio::task::yield_now().await;
                }
                Err(error) => break Err(error),
            }
        }
    })
    .await
    .expect("capacity did not resume within one second")
    .expect("resumed login transport");
    let mut resumed_stream = resumed_response.into_inner();
    let resumed_started = timeout(Duration::from_secs(1), resumed_stream.message())
        .await
        .expect("resumed started event deadline")
        .expect("resumed started event transport")
        .expect("resumed started event");
    assert_eq!(resumed_started.status, AuthStatus::Idle as i32);

    drop(resumed_stream);
    drop(resumed_client);
    drop(streams);
    drop(clients);
}

#[tokio::test]
async fn cancel_rpc_stops_pending_acquire_and_keeps_session_deleted() {
    // Given: a real Login stream blocked in the browser factory.
    let acquire_count = Arc::new(AtomicUsize::new(0));
    let browser = Arc::new(PendingBrowserFactory {
        acquire_count: acquire_count.clone(),
    });
    let context = test_context_with_browser(browser, Duration::from_secs(300)).await;
    let server = start_test_server(&context, Some("test-api-key".to_string())).await;
    let mut client = AuthServiceClient::connect(server.endpoint.clone())
        .await
        .expect("connect login client");
    let mut stream = client
        .login(with_key(LoginRequest {
            platform: Platform::Instagram as i32,
        }))
        .await
        .expect("login stream admitted")
        .into_inner();
    let started = timeout(Duration::from_secs(1), stream.message())
        .await
        .expect("started event deadline")
        .expect("started event transport")
        .expect("started event");
    wait_for_acquires(&acquire_count, 1).await;
    let session_id = started.session_id.clone();

    // When: the explicit Cancel RPC deletes the pending session.
    let cancelled = client
        .cancel(with_key(CancelRequest {
            session_id: session_id.clone(),
        }))
        .await
        .expect("cancel transport")
        .into_inner();
    assert!(cancelled.success);

    // Then: the pending acquire terminates within one second with Failed and no cookies.
    let terminal = timeout(Duration::from_secs(1), stream.message())
        .await
        .expect("cancelled terminal event deadline")
        .expect("cancelled terminal event transport")
        .expect("cancelled terminal event");
    assert_eq!(terminal.session_id, session_id);
    assert_eq!(terminal.status, AuthStatus::Failed as i32);
    assert!(terminal.cookies.is_empty());
    assert!(timeout(Duration::from_secs(1), stream.message())
        .await
        .expect("stream completion deadline")
        .expect("stream completion transport")
        .is_none());

    // Then: the failed terminal update does not resurrect the deleted session.
    let error = client
        .get_status(with_key(StatusRequest { session_id }))
        .await
        .expect_err("cancelled session must remain deleted");
    assert_eq!(error.code(), tonic::Code::NotFound);
}
