use crate::domain::session::{Session, SessionState};
use crate::domain::Platform;
use crate::ports::browser::{BrowserSessionFactory, PlatformDriver};
use crate::ports::repository::{CookieRepository, SessionRepository};
use crate::ports::snapshot::SnapshotSink;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum LoginEvent {
    Started(Session),
    Final(Session),
}

pub struct LoginUseCase {
    sessions: Arc<dyn SessionRepository>,
    cookies: Arc<dyn CookieRepository>,
    browser: Arc<dyn BrowserSessionFactory>,
    driver: Arc<dyn PlatformDriver>,
    snapshot: Arc<dyn SnapshotSink>,
}

impl LoginUseCase {
    pub fn new(
        sessions: Arc<dyn SessionRepository>,
        cookies: Arc<dyn CookieRepository>,
        browser: Arc<dyn BrowserSessionFactory>,
        driver: Arc<dyn PlatformDriver>,
        snapshot: Arc<dyn SnapshotSink>,
    ) -> Self {
        Self {
            sessions,
            cookies,
            browser,
            driver,
            snapshot,
        }
    }

    pub async fn execute(
        &self,
        platform: Platform,
        username: String,
        password: String,
        tx: mpsc::Sender<LoginEvent>,
    ) {
        let id = uuid::Uuid::new_v4().to_string();
        let initial = Session::new(id, platform.as_str().to_string());

        let mut session = match self.sessions.create(initial).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to create session: {e}");
                return;
            }
        };

        let _ = tx.send(LoginEvent::Started(session.clone())).await;

        let browser_session = match self.browser.acquire().await {
            Ok(b) => b,
            Err(e) => {
                session.transition(SessionState::Failed, Some(format!("Browser error: {e}")));
                let _ = self.sessions.update(&session).await;
                let _ = tx.send(LoginEvent::Final(session)).await;
                return;
            }
        };

        let page = match browser_session.new_page().await {
            Ok(p) => p,
            Err(e) => {
                session.transition(
                    SessionState::Failed,
                    Some(format!("Failed to open page: {e}")),
                );
                let _ = self.sessions.update(&session).await;
                let _ = tx.send(LoginEvent::Final(session)).await;
                return;
            }
        };

        if let Err(e) = self
            .driver
            .login(
                &*page,
                platform,
                &username,
                &password,
                &mut session,
                &*self.snapshot,
            )
            .await
        {
            session.transition(SessionState::Failed, Some(e.to_string()));
        }

        let _ = self.sessions.update(&session).await;

        if session.state == SessionState::Connected {
            if let Err(e) = self.cookies.save(&session.platform, &session.cookies).await {
                tracing::warn!("Failed to persist cookies after login: {e}");
            }
        }

        let _ = tx.send(LoginEvent::Final(session)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result as AppResult;
    use crate::domain::session::Cookie;
    use crate::ports::browser::{
        MockBrowserSession, MockBrowserSessionFactory, MockPageDriver, PageDriver, PlatformDriver,
    };
    use crate::ports::repository::{MockCookieRepository, MockSessionRepository};
    use crate::ports::snapshot::{MockSnapshotSink, SnapshotSink};
    use async_trait::async_trait;

    /// Test double — sets the session to `Connected` with a sample cookie.
    struct ConnectingDriver;
    #[async_trait]
    impl PlatformDriver for ConnectingDriver {
        async fn login<'a>(
            &'a self,
            _page: &'a dyn PageDriver,
            _platform: Platform,
            _username: &'a str,
            _password: &'a str,
            session: &'a mut Session,
            _snapshot: &'a dyn SnapshotSink,
        ) -> AppResult<()> {
            session.cookies = vec![Cookie {
                name: "sessionid".into(),
                value: "abc".into(),
                domain: ".instagram.com".into(),
                path: "/".into(),
                expires: None,
                http_only: true,
                secure: true,
            }];
            session.transition(SessionState::Connected, Some("ok".into()));
            Ok(())
        }
        async fn submit_2fa<'a>(
            &'a self,
            _page: &'a dyn PageDriver,
            _platform: Platform,
            _code: &'a str,
            _session: &'a mut Session,
            _snapshot: &'a dyn SnapshotSink,
        ) -> AppResult<()> {
            Ok(())
        }
    }

    /// Test double — sets the session to `Failed`.
    struct FailingDriver;
    #[async_trait]
    impl PlatformDriver for FailingDriver {
        async fn login<'a>(
            &'a self,
            _page: &'a dyn PageDriver,
            _platform: Platform,
            _username: &'a str,
            _password: &'a str,
            session: &'a mut Session,
            _snapshot: &'a dyn SnapshotSink,
        ) -> AppResult<()> {
            session.transition(SessionState::Failed, Some("bad creds".into()));
            Ok(())
        }
        async fn submit_2fa<'a>(
            &'a self,
            _page: &'a dyn PageDriver,
            _platform: Platform,
            _code: &'a str,
            _session: &'a mut Session,
            _snapshot: &'a dyn SnapshotSink,
        ) -> AppResult<()> {
            Ok(())
        }
    }

    fn build_login_use_case(
        sessions: MockSessionRepository,
        cookies: MockCookieRepository,
        browser: MockBrowserSessionFactory,
        driver: Arc<dyn PlatformDriver>,
        snapshot: MockSnapshotSink,
    ) -> LoginUseCase {
        LoginUseCase::new(
            Arc::new(sessions),
            Arc::new(cookies),
            Arc::new(browser),
            driver,
            Arc::new(snapshot),
        )
    }

    fn happy_browser() -> MockBrowserSessionFactory {
        let mut factory = MockBrowserSessionFactory::new();
        factory.expect_acquire().return_once(|| {
            let mut session = MockBrowserSession::new();
            session.expect_new_page().return_once(|| {
                let page = MockPageDriver::new();
                Ok(Box::new(page))
            });
            Ok(Box::new(session))
        });
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

        let snapshot = MockSnapshotSink::new();

        let uc = build_login_use_case(
            sessions,
            cookies,
            happy_browser(),
            Arc::new(ConnectingDriver),
            snapshot,
        );

        let (tx, mut rx) = mpsc::channel(8);
        uc.execute(Platform::Instagram, "u".into(), "p".into(), tx)
            .await;

        let started = rx.recv().await.expect("started event");
        let final_evt = rx.recv().await.expect("final event");
        assert!(matches!(started, LoginEvent::Started(_)));
        match final_evt {
            LoginEvent::Final(s) => {
                assert_eq!(s.state, SessionState::Connected);
                assert_eq!(s.cookies.len(), 1);
            }
            _ => panic!("expected Final"),
        }
    }

    #[tokio::test]
    async fn login_failure_does_not_save_cookies() {
        let mut sessions = MockSessionRepository::new();
        sessions.expect_create().returning(Ok);
        sessions.expect_update().returning(|_| Ok(()));

        let mut cookies = MockCookieRepository::new();
        // crucial: cookies.save must NOT be called on failure
        cookies.expect_save().times(0);

        let snapshot = MockSnapshotSink::new();

        let uc = build_login_use_case(
            sessions,
            cookies,
            happy_browser(),
            Arc::new(FailingDriver),
            snapshot,
        );

        let (tx, mut rx) = mpsc::channel(8);
        uc.execute(Platform::Instagram, "u".into(), "p".into(), tx)
            .await;
        let _ = rx.recv().await;
        let final_evt = rx.recv().await.unwrap();
        match final_evt {
            LoginEvent::Final(s) => assert_eq!(s.state, SessionState::Failed),
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

        let snapshot = MockSnapshotSink::new();

        let uc = build_login_use_case(
            sessions,
            cookies,
            browser,
            Arc::new(ConnectingDriver), // never called
            snapshot,
        );

        let (tx, mut rx) = mpsc::channel(8);
        uc.execute(Platform::Instagram, "u".into(), "p".into(), tx)
            .await;
        let _ = rx.recv().await;
        let final_evt = rx.recv().await.unwrap();
        match final_evt {
            LoginEvent::Final(s) => {
                assert_eq!(s.state, SessionState::Failed);
                assert!(s.message.unwrap_or_default().contains("Browser error"));
            }
            _ => panic!("expected Final"),
        }
    }
}
