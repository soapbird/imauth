use super::*;
use crate::domain::session::Cookie;
use crate::ports::browser::{MockBrowserSession, MockBrowserSessionFactory};
use crate::ports::repository::{MockCookieRepository, MockSessionRepository};
use crate::Result as AppResult;
use async_trait::async_trait;

/// Test double — returns a session cookie on the first poll.
struct ImmediateCookiePageDriver;
#[async_trait]
impl crate::ports::browser::PageDriver for ImmediateCookiePageDriver {
    async fn navigate(&self, _url: &str, _timeout_secs: u64) -> AppResult<()> {
        Ok(())
    }
    async fn get_cookies(&self) -> AppResult<Vec<Cookie>> {
        Ok(vec![Cookie {
            name: "sessionid".into(),
            value: "abc".into(),
            domain: ".instagram.com".into(),
            path: "/".into(),
            expires: None,
            http_only: true,
            secure: true,
        }])
    }
    async fn screenshot(&self) -> AppResult<Vec<u8>> {
        Ok(vec![])
    }
    async fn content_html(&self) -> AppResult<String> {
        Ok(String::new())
    }
    async fn close(&self) -> AppResult<()> {
        Ok(())
    }
}

fn build_login_use_case(
    sessions: MockSessionRepository,
    cookies: MockCookieRepository,
    browser: MockBrowserSessionFactory,
) -> LoginUseCase {
    LoginUseCase::new(
        Arc::new(sessions),
        Arc::new(cookies),
        Arc::new(browser),
        Duration::from_secs(5),
    )
}

fn happy_browser_with_page(
    page: Box<dyn crate::ports::browser::PageDriver>,
) -> MockBrowserSessionFactory {
    let mut factory = MockBrowserSessionFactory::new();
    factory.expect_acquire().return_once(|| {
        let mut session = MockBrowserSession::new();
        session.expect_new_page().return_once(move || Ok(page));
        session.expect_close().times(1).returning(|| Ok(()));
        session.expect_wait_disconnected().returning(|| ());
        session
            .expect_viewer_url()
            .returning(|| "http://localhost:6101/index.html".to_string());
        Ok(Box::new(session))
    });
    factory
        .expect_viewer_url()
        .returning(|| Some("http://localhost:6101/index.html".to_string()));
    factory
}

#[tokio::test]
async fn login_connected_path_saves_cookies_and_emits_final_event() {
    let mut sessions = MockSessionRepository::new();
    sessions.expect_create().returning(Ok);
    sessions
        .expect_get()
        .returning(|_| Ok(Some(Session::new("active".into(), "instagram".into()))));
    sessions.expect_update().returning(|_| Ok(()));

    let mut cookies = MockCookieRepository::new();
    cookies
        .expect_save_login()
        .withf(|p, c| p.platform == "instagram" && c.len() == 1)
        .times(1)
        .returning(|_, _| Ok(()));

    let page = Box::new(ImmediateCookiePageDriver);
    let uc = build_login_use_case(sessions, cookies, happy_browser_with_page(page));

    let (tx, mut rx) = mpsc::channel(8);
    uc.execute(Platform::Instagram, tx).await;

    let started = rx.recv().await.expect("started event");
    assert!(matches!(started, LoginEvent::Started(_)));

    let waiting = rx.recv().await.expect("waiting event");
    match &waiting {
        LoginEvent::WaitingForUser(_, url) => {
            assert!(url.ends_with("/index.html"));
        }
        _ => panic!("expected WaitingForUser, got {:?}", waiting),
    }

    let final_evt = rx.recv().await.expect("final event");
    match final_evt {
        LoginEvent::Final(s, cookies) => {
            assert_eq!(s.state, SessionState::Connected);
            assert_eq!(cookies.len(), 1);
        }
        _ => panic!("expected Final"),
    }
}

#[tokio::test]
async fn login_browser_acquire_failure_emits_failed_final_event() {
    let mut sessions = MockSessionRepository::new();
    sessions.expect_create().returning(Ok);
    sessions.expect_update().returning(|_| Ok(()));

    let mut cookies = MockCookieRepository::new();
    cookies.expect_save_login().times(0);

    let mut browser = MockBrowserSessionFactory::new();
    browser
        .expect_acquire()
        .return_once(|| Err(crate::ImauthError::Browser("no cdp".into())));
    browser.expect_viewer_url().returning(|| None);

    let uc = build_login_use_case(sessions, cookies, browser);

    let (tx, mut rx) = mpsc::channel(8);
    uc.execute(Platform::Instagram, tx).await;
    let _ = rx.recv().await;
    let final_evt = rx.recv().await.unwrap();
    match final_evt {
        LoginEvent::Final(s, cookies) => {
            assert_eq!(s.state, SessionState::Failed);
            assert!(s.message.unwrap_or_default().contains("Browser error"));
            assert!(cookies.is_empty());
        }
        _ => panic!("expected Final"),
    }
}

#[tokio::test]
async fn deleted_session_cancels_login_and_closes_page() {
    let mut sessions = MockSessionRepository::new();
    sessions.expect_create().returning(Ok);
    sessions.expect_get().times(1).returning(|_| Ok(None));
    sessions.expect_update().returning(|_| Ok(()));

    let mut cookies = MockCookieRepository::new();
    cookies.expect_save_login().times(0);

    let mut page = crate::ports::browser::MockPageDriver::new();
    page.expect_navigate().return_once(|_, _| Ok(()));
    page.expect_get_cookies().times(0);
    page.expect_close().times(0);
    let uc = build_login_use_case(sessions, cookies, happy_browser_with_page(Box::new(page)));

    let (tx, mut rx) = mpsc::channel(8);
    uc.execute(Platform::Instagram, tx).await;

    assert!(matches!(rx.recv().await, Some(LoginEvent::Started(_))));
    assert!(matches!(
        rx.recv().await,
        Some(LoginEvent::WaitingForUser(_, _))
    ));
    match rx.recv().await {
        Some(LoginEvent::Final(session, cookies)) => {
            assert_eq!(session.state, SessionState::Failed);
            assert_eq!(session.message.as_deref(), Some("Login cancelled"));
            assert!(cookies.is_empty());
        }
        other => panic!("expected cancelled final event, got {other:?}"),
    }
}

struct PendingBrowserFactory(Arc<tokio::sync::Notify>);

#[async_trait]
impl BrowserSessionFactory for PendingBrowserFactory {
    async fn acquire(&self) -> AppResult<Box<dyn crate::ports::browser::BrowserSession>> {
        self.0.notify_one();
        std::future::pending().await
    }

    fn viewer_url(&self) -> Option<String> {
        None
    }
}

#[tokio::test(start_paused = true)]
async fn regression_disconnected_client_cancels_pending_browser_acquisition() {
    // Given: a login blocked waiting for a browser slot.
    let entered = Arc::new(tokio::sync::Notify::new());
    let mut sessions = MockSessionRepository::new();
    sessions.expect_create().returning(Ok);
    sessions.expect_update().returning(|_| Ok(()));
    sessions
        .expect_get()
        .returning(|_| Ok(Some(Session::new("active".into(), "instagram".into()))));
    let uc = LoginUseCase::new(
        Arc::new(sessions),
        Arc::new(MockCookieRepository::new()),
        Arc::new(PendingBrowserFactory(entered.clone())),
        Duration::from_secs(300),
    );
    let (tx, mut rx) = mpsc::channel(8);
    let mut task = tokio::spawn(async move { uc.execute(Platform::Instagram, tx).await });
    assert!(matches!(rx.recv().await, Some(LoginEvent::Started(_))));
    entered.notified().await;

    // When: the requesting client disconnects while acquisition is pending.
    drop(rx);
    let result = tokio::time::timeout(Duration::from_secs(1), &mut task).await;
    task.abort();

    // Then: the login task stops without waiting for the acquisition timeout.
    assert!(
        result.is_ok(),
        "disconnected login remained blocked in acquire"
    );
}

#[tokio::test(start_paused = true)]
async fn regression_cookie_persistence_failure_never_reports_connected() {
    // Given: the browser has a session cookie but its persistent store fails.
    let mut sessions = MockSessionRepository::new();
    sessions.expect_create().returning(Ok);
    sessions
        .expect_get()
        .returning(|_| Ok(Some(Session::new("active".into(), "instagram".into()))));
    sessions.expect_update().returning(|_| Ok(()));
    let mut cookies = MockCookieRepository::new();
    cookies
        .expect_save_login()
        .returning(|_, _| Err(crate::ImauthError::Database("disk full".into())));
    let uc = build_login_use_case(
        sessions,
        cookies,
        happy_browser_with_page(Box::new(ImmediateCookiePageDriver)),
    );
    let (tx, mut rx) = mpsc::channel(8);

    // When: login reaches the cookie persistence step.
    uc.execute(Platform::Instagram, tx).await;
    let mut final_event = None;
    while let Some(event) = rx.recv().await {
        if let LoginEvent::Final(session, cookies) = event {
            final_event = Some((session, cookies));
        }
    }

    // Then: the caller receives failure, not an apparently durable login.
    let (session, cookies) = final_event.expect("terminal event");
    assert_eq!(session.state, SessionState::Failed);
    assert!(cookies.is_empty());
}
