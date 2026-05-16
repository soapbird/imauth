use crate::application::active_session::{ActiveLoginSession, ActiveSessionRegistry};
use crate::domain::session::{Cookie, Session, SessionState};
use crate::domain::Platform;
use crate::ports::browser::{BrowserSessionFactory, PlatformDriver};
use crate::ports::repository::{CookieRepository, SessionRepository};
use crate::ports::snapshot::SnapshotSink;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum LoginEvent {
    Started(Session),
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
    active: Arc<ActiveSessionRegistry>,
    drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
    snapshot: Arc<dyn SnapshotSink>,
}

impl LoginUseCase {
    pub fn new(
        sessions: Arc<dyn SessionRepository>,
        cookies: Arc<dyn CookieRepository>,
        browser: Arc<dyn BrowserSessionFactory>,
        active: Arc<ActiveSessionRegistry>,
        drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
        snapshot: Arc<dyn SnapshotSink>,
    ) -> Self {
        Self {
            sessions,
            cookies,
            browser,
            active,
            drivers,
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
                // Use an empty session_id so callers don't get a UUID that
                // doesn't exist in the DB. Honest signal: "this session never
                // persisted, GetStatus will return NotFound".
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
            // Client already dropped the receiver before we sent anything.
            tracing::info!(session_id = %session.id, "login stream cancelled before Started; aborting");
            return;
        }

        if tx.is_closed() {
            tracing::info!(session_id = %session.id, "login cancelled after session create; aborting before browser acquire");
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

        let driver = match self.drivers.get(&platform) {
            Some(d) => d,
            None => {
                session.transition(
                    SessionState::Failed,
                    Some(format!("No driver for platform {}", platform.as_str())),
                );
                let _ = self.sessions.update(&session).await;
                send_or_warn(&tx, LoginEvent::Final(session, vec![]), "no-driver").await;
                return;
            }
        };

        let cookies = match driver
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
            Ok(cookies) => cookies,
            Err(e) => {
                session.transition(SessionState::Failed, Some(e.to_string()));
                Vec::new()
            }
        };

        let _ = self.sessions.update(&session).await;

        if session.state == SessionState::Connected {
            if let Err(e) = self.cookies.save(&session.platform, &cookies).await {
                tracing::warn!("Failed to persist cookies after login: {e}");
            }
        }

        // If the driver left the session in a state that needs follow-up input
        // (2FA, captcha), pin the browser+page to this session_id so the
        // matching Submit2Fa / SubmitCaptcha calls drive THIS browser, not an
        // arbitrary one from the pool. Drop on terminal states releases the
        // browser back to the pool immediately.
        if matches!(
            session.state,
            SessionState::Needs2Fa | SessionState::NeedsCaptcha
        ) {
            let entry = ActiveLoginSession {
                browser: browser_session,
                page,
                platform,
            };
            self.active.register(session.id.clone(), entry).await;
        } else {
            // Connected / Failed / etc — let the values fall out of scope so
            // the BrowserSession Drop returns the browser to the pool.
            drop(page);
            drop(browser_session);
        }

        send_or_warn(&tx, LoginEvent::Final(session, cookies), "final").await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::Cookie;
    use crate::ports::browser::{
        MockBrowserSession, MockBrowserSessionFactory, MockPageDriver, PageDriver, PlatformDriver,
    };
    use crate::ports::repository::{MockCookieRepository, MockSessionRepository};
    use crate::ports::snapshot::{MockSnapshotSink, SnapshotSink};
    use crate::Result as AppResult;
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
        ) -> AppResult<Vec<Cookie>> {
            session.transition(SessionState::Connected, Some("ok".into()));
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
        async fn submit_2fa<'a>(
            &'a self,
            _page: &'a dyn PageDriver,
            _platform: Platform,
            _code: &'a str,
            _session: &'a mut Session,
            _snapshot: &'a dyn SnapshotSink,
        ) -> AppResult<Vec<Cookie>> {
            Ok(vec![])
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
        ) -> AppResult<Vec<Cookie>> {
            session.transition(SessionState::Failed, Some("bad creds".into()));
            Ok(vec![])
        }
        async fn submit_2fa<'a>(
            &'a self,
            _page: &'a dyn PageDriver,
            _platform: Platform,
            _code: &'a str,
            _session: &'a mut Session,
            _snapshot: &'a dyn SnapshotSink,
        ) -> AppResult<Vec<Cookie>> {
            Ok(vec![])
        }
    }

    fn build_login_use_case(
        sessions: MockSessionRepository,
        cookies: MockCookieRepository,
        browser: MockBrowserSessionFactory,
        drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
        snapshot: MockSnapshotSink,
    ) -> LoginUseCase {
        LoginUseCase::new(
            Arc::new(sessions),
            Arc::new(cookies),
            Arc::new(browser),
            Arc::new(ActiveSessionRegistry::new()),
            drivers,
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

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_login_use_case(sessions, cookies, happy_browser(), drivers, snapshot);

        let (tx, mut rx) = mpsc::channel(8);
        uc.execute(Platform::Instagram, "u".into(), "p".into(), tx)
            .await;

        let started = rx.recv().await.expect("started event");
        let final_evt = rx.recv().await.expect("final event");
        assert!(matches!(started, LoginEvent::Started(_)));
        match final_evt {
            LoginEvent::Final(s, cookies) => {
                assert_eq!(s.state, SessionState::Connected);
                assert_eq!(cookies.len(), 1);
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

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(FailingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_login_use_case(sessions, cookies, happy_browser(), drivers, snapshot);

        let (tx, mut rx) = mpsc::channel(8);
        uc.execute(Platform::Instagram, "u".into(), "p".into(), tx)
            .await;
        let _ = rx.recv().await;
        let final_evt = rx.recv().await.unwrap();
        match final_evt {
            LoginEvent::Final(s, cookies) => {
                assert_eq!(s.state, SessionState::Failed);
                assert!(cookies.is_empty());
            }
            _ => panic!("expected Final"),
        }
    }

    #[tokio::test]
    async fn login_session_create_failure_emits_failed_final_event() {
        let mut sessions = MockSessionRepository::new();
        sessions
            .expect_create()
            .return_once(|_| Err(crate::ImauthError::Browser("db down".into())));

        let mut cookies = MockCookieRepository::new();
        cookies.expect_save().times(0);

        let browser = MockBrowserSessionFactory::new();
        let snapshot = MockSnapshotSink::new();

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_login_use_case(sessions, cookies, browser, drivers, snapshot);

        let (tx, mut rx) = mpsc::channel(8);
        uc.execute(Platform::Instagram, "u".into(), "p".into(), tx)
            .await;

        let final_evt = rx.recv().await.expect("client must see a terminal event");
        match final_evt {
            LoginEvent::Final(s, cookies) => {
                assert_eq!(s.state, SessionState::Failed);
                // Empty session_id signals "never persisted" so the client
                // can distinguish DB-failure from a real session that later
                // transitioned to Failed.
                assert_eq!(s.id, "", "unpersisted session must carry an empty id");
                assert_eq!(s.platform, Platform::Instagram.as_str());
                assert_eq!(
                    s.message.as_deref(),
                    Some("Failed to create session"),
                    "operators rely on this exact wording to debug DB failures"
                );
                assert!(cookies.is_empty());
            }
            _ => panic!("expected Final, got Started — clients would see empty stream"),
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

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_login_use_case(sessions, cookies, browser, drivers, snapshot);

        let (tx, mut rx) = mpsc::channel(8);
        uc.execute(Platform::Instagram, "u".into(), "p".into(), tx)
            .await;
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
