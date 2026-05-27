use crate::domain::auth::classify_auth_state;
use crate::domain::session::{Cookie, Session, SessionState};
use crate::domain::Platform;
use crate::ports::browser::BrowserSessionFactory;
use crate::ports::repository::{CookieRepository, SessionRepository};
use metrics::histogram;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

struct LoginTimer {
    start: std::time::Instant,
    platform: String,
}

impl LoginTimer {
    fn new(platform: &str) -> Self {
        Self {
            start: std::time::Instant::now(),
            platform: platform.to_string(),
        }
    }
}

impl Drop for LoginTimer {
    fn drop(&mut self) {
        histogram!(
            "login_duration_seconds",
            "platform" => self.platform.clone()
        )
        .record(self.start.elapsed().as_secs_f64());
    }
}

#[derive(Debug, Clone)]
pub enum LoginEvent {
    Started(Session),
    WaitingForUser(Session, String), // (session, viewer_url)
    Final(Session, Vec<Cookie>),
}

/// Send a LoginEvent, logging when the receiver is gone instead of silently
/// dropping the event. Returns `true` when the channel is closed so callers
/// can short-circuit the rest of the login pipeline.
async fn send_or_warn(
    tx: &mpsc::Sender<LoginEvent>,
    event: LoginEvent,
    stage: &'static str,
) -> bool {
    if let Err(e) = tx.send(event).await {
        tracing::warn!(stage = stage, "login event dropped: receiver gone ({e})");
        true
    } else {
        false
    }
}

pub struct LoginUseCase {
    sessions: Arc<dyn SessionRepository>,
    cookies: Arc<dyn CookieRepository>,
    browser: Arc<dyn BrowserSessionFactory>,
    login_timeout: Duration,
}

impl LoginUseCase {
    pub fn new(
        sessions: Arc<dyn SessionRepository>,
        cookies: Arc<dyn CookieRepository>,
        browser: Arc<dyn BrowserSessionFactory>,
        login_timeout: Duration,
    ) -> Self {
        Self {
            sessions,
            cookies,
            browser,
            login_timeout,
        }
    }

    pub async fn execute(
        &self,
        platform: Platform,
        tx: mpsc::Sender<LoginEvent>,
    ) {
        let _timer = LoginTimer::new(platform.as_str());
        let id = uuid::Uuid::new_v4().to_string();
        let initial = Session::new(id, platform.as_str().to_string());

        let mut session = match self.sessions.create(initial).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to create session: {e}");
                let mut failed = Session::new(String::new(), platform.as_str().to_string());
                failed.transition(
                    SessionState::Failed,
                    Some("Failed to create session".into()),
                );
                send_or_warn(&tx, LoginEvent::Final(failed, vec![]), "session-create-fail").await;
                return;
            }
        };

        if send_or_warn(&tx, LoginEvent::Started(session.clone()), "started").await {
            return;
        }

        if tx.is_closed() {
            tracing::info!(session_id = %session.id, "login cancelled after session create");
            return;
        }

        let browser_session = match self.browser.acquire().await {
            Ok(b) => b,
            Err(e) => {
                session.transition(SessionState::Failed, Some(format!("Browser error: {e}")));
                let _ = self.sessions.update(&session).await;
                send_or_warn(&tx, LoginEvent::Final(session, vec![]), "browser-acquire-fail").await;
                return;
            }
        };

        if tx.is_closed() {
            tracing::info!(session_id = %session.id, "login cancelled after browser acquire");
            return;
        }

        let page = match browser_session.new_page().await {
            Ok(p) => p,
            Err(e) => {
                session.transition(
                    SessionState::Failed,
                    Some(format!("Failed to open page: {e}")),
                );
                let _ = self.sessions.update(&session).await;
                send_or_warn(&tx, LoginEvent::Final(session, vec![]), "page-open-fail").await;
                return;
            }
        };

        session.transition(
            SessionState::Loading,
            Some(format!("Opening {} login page...", platform.as_str())),
        );
        let _ = self.sessions.update(&session).await;

        if let Err(e) = page.set_mobile_viewport().await {
            tracing::warn!(session_id = %session.id, "failed to set mobile viewport: {e}");
        }

        if let Err(e) = page.navigate(platform.login_url(), 30).await {
            session.transition(
                SessionState::Failed,
                Some(format!("Navigation error: {e}")),
            );
            let _ = self.sessions.update(&session).await;
            send_or_warn(&tx, LoginEvent::Final(session, vec![]), "navigate-fail").await;
            return;
        }

        let viewer_url = browser_session.viewer_url();

        session.transition(
            SessionState::WaitingForUser,
            Some("Waiting for user to log in via browser".into()),
        );
        let _ = self.sessions.update(&session).await;

        if send_or_warn(
            &tx,
            LoginEvent::WaitingForUser(session.clone(), viewer_url.clone()),
            "waiting-for-user",
        )
        .await
        {
            return;
        }

        // Poll cookies until session cookie appears or timeout
        let deadline = tokio::time::Instant::now() + self.login_timeout;
        let mut cookies = Vec::new();

        loop {
            if tx.is_closed() {
                tracing::info!(session_id = %session.id, "login cancelled during cookie polling");
                return;
            }

            if tokio::time::Instant::now() >= deadline {
                session.transition(
                    SessionState::Failed,
                    Some("Login timed out — no session cookie detected".into()),
                );
                break;
            }

            tokio::time::sleep(Duration::from_secs(2)).await;

            match page.get_cookies().await {
                Ok(raw_cookies) => {
                    match classify_auth_state(raw_cookies, platform) {
                        crate::domain::auth::AuthCheckpoint::Connected(filtered) => {
                            session.transition(
                                SessionState::Connected,
                                Some("Login successful".into()),
                            );
                            cookies = filtered;
                            break;
                        }
                        crate::domain::auth::AuthCheckpoint::Pending => {}
                    }
                }
                Err(e) => {
                    tracing::warn!(session_id = %session.id, "cookie poll error: {e}");
                }
            }
        }

        let _ = self.sessions.update(&session).await;

        if session.state == SessionState::Connected {
            if let Err(e) = self.cookies.save(&session.platform, &cookies).await {
                tracing::warn!("Failed to persist cookies after login: {e}");
            }
        }

        if matches!(session.state, SessionState::WaitingForUser) {
            // If still waiting but we broke out (shouldn't happen), mark failed
            session.transition(SessionState::Failed, Some("Login did not complete".into()));
        }

        // For WaitingForUser that transitioned to Connected/Failed, close page and drop browser.
        // The active session registry is no longer needed for 2FA since login is user-driven.
        if let Err(e) = page.close().await {
            tracing::warn!(session_id = %session.id, "failed to close page: {e}");
        }
        drop(page);
        drop(browser_session);

        send_or_warn(&tx, LoginEvent::Final(session, cookies), "final").await;
    }
}

#[cfg(test)]
mod tests {
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
        async fn set_mobile_viewport(&self) -> AppResult<()> {
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

    fn happy_browser_with_page(page: Box<dyn crate::ports::browser::PageDriver>) -> MockBrowserSessionFactory {
        let mut factory = MockBrowserSessionFactory::new();
        factory.expect_acquire().return_once(|| {
            let mut session = MockBrowserSession::new();
            session.expect_new_page().return_once(move || Ok(page));
            session.expect_viewer_url().returning(|| "http://localhost:6101/index.html".to_string());
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
        sessions.expect_update().returning(|_| Ok(()));

        let mut cookies = MockCookieRepository::new();
        cookies
            .expect_save()
            .withf(|p, c| p == "instagram" && c.len() == 1)
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
        cookies.expect_save().times(0);

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
}
