use crate::application::active_session::ActiveSessionRegistry;
use crate::domain::session::{Cookie, Session, SessionState};
use crate::domain::Platform;
use crate::ports::browser::PlatformDriver;
use crate::ports::repository::{CookieRepository, SessionRepository};
use crate::ports::snapshot::SnapshotSink;
use crate::ImauthError;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Submit2FaUseCase {
    sessions: Arc<dyn SessionRepository>,
    cookies: Arc<dyn CookieRepository>,
    active: Arc<ActiveSessionRegistry>,
    drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
    snapshot: Arc<dyn SnapshotSink>,
}

impl Submit2FaUseCase {
    pub fn new(
        sessions: Arc<dyn SessionRepository>,
        cookies: Arc<dyn CookieRepository>,
        active: Arc<ActiveSessionRegistry>,
        drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
        snapshot: Arc<dyn SnapshotSink>,
    ) -> Self {
        Self {
            sessions,
            cookies,
            active,
            drivers,
            snapshot,
        }
    }

    pub async fn execute(&self, session_id: &str, code: &str) -> Result<(Session, Vec<Cookie>)> {
        let mut session = self
            .sessions
            .get(session_id)
            .await?
            .ok_or_else(|| ImauthError::NotFound("Session not found".into()))?;

        let platform = Platform::from_str(&session.platform).ok_or_else(|| {
            ImauthError::Platform(format!("Unknown platform {}", session.platform))
        })?;

        let driver = self.drivers.get(&platform).ok_or_else(|| {
            ImauthError::Platform(format!("No driver for platform {}", platform.as_str()))
        })?;

        // Take the browser+page pair that Login left bound to this session.
        // Drop returns the browser to the pool whether submit succeeds or
        // errors. Without this binding, submit_2fa was acquiring any browser
        // from the pool and typing the code into the wrong tab (Red Team
        // finding: cross-session credential leak).
        let bound = self.active.take(session_id).await.ok_or_else(|| {
            ImauthError::Browser(
                "No browser bound to this session (login may have already failed or been cancelled)".into(),
            )
        })?;

        if bound.platform != platform {
            return Err(ImauthError::Platform(format!(
                "Session {} platform mismatch: bound={:?}, session={:?}",
                session_id, bound.platform, platform
            )));
        }

        let cookies = driver
            .submit_2fa(&*bound.page, platform, code, &mut session, &*self.snapshot)
            .await?;

        self.sessions.update(&session).await?;

        if session.state == SessionState::Connected {
            self.cookies.save(&session.platform, &cookies).await?;
        }

        Ok((session, cookies))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::active_session::ActiveLoginSession;
    use crate::domain::session::Cookie;
    use crate::ports::browser::{MockBrowserSession, MockPageDriver, PageDriver, PlatformDriver};
    use crate::ports::repository::{MockCookieRepository, MockSessionRepository};
    use crate::ports::snapshot::{MockSnapshotSink, SnapshotSink};
    use crate::Result as AppResult;
    use async_trait::async_trait;

    struct ConnectingDriver;
    #[async_trait]
    impl PlatformDriver for ConnectingDriver {
        async fn login<'a>(
            &'a self,
            _: &'a dyn PageDriver,
            _: Platform,
            _: &'a str,
            _: &'a str,
            _: &'a mut Session,
            _: &'a dyn SnapshotSink,
        ) -> AppResult<Vec<Cookie>> {
            Ok(vec![])
        }
        async fn submit_2fa<'a>(
            &'a self,
            _: &'a dyn PageDriver,
            _: Platform,
            _: &'a str,
            session: &'a mut Session,
            _: &'a dyn SnapshotSink,
        ) -> AppResult<Vec<Cookie>> {
            session.transition(SessionState::Connected, Some("ok".into()));
            Ok(vec![Cookie {
                name: "sessionid".into(),
                value: "v".into(),
                domain: ".instagram.com".into(),
                path: "/".into(),
                expires: None,
                http_only: true,
                secure: true,
            }])
        }
    }

    struct FailingDriver;
    #[async_trait]
    impl PlatformDriver for FailingDriver {
        async fn login<'a>(
            &'a self,
            _: &'a dyn PageDriver,
            _: Platform,
            _: &'a str,
            _: &'a str,
            _: &'a mut Session,
            _: &'a dyn SnapshotSink,
        ) -> AppResult<Vec<Cookie>> {
            Ok(vec![])
        }
        async fn submit_2fa<'a>(
            &'a self,
            _: &'a dyn PageDriver,
            _: Platform,
            _: &'a str,
            session: &'a mut Session,
            _: &'a dyn SnapshotSink,
        ) -> AppResult<Vec<Cookie>> {
            session.transition(SessionState::Failed, Some("bad code".into()));
            Ok(vec![])
        }
    }

    fn build_uc(
        sessions: MockSessionRepository,
        cookies: MockCookieRepository,
        active: Arc<ActiveSessionRegistry>,
        drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
        snapshot: MockSnapshotSink,
    ) -> Submit2FaUseCase {
        Submit2FaUseCase::new(
            Arc::new(sessions),
            Arc::new(cookies),
            active,
            drivers,
            Arc::new(snapshot),
        )
    }

    async fn register_active_session(
        registry: &ActiveSessionRegistry,
        session_id: &str,
        platform: Platform,
    ) {
        let entry = ActiveLoginSession {
            browser: Box::new(MockBrowserSession::new()),
            page: Box::new(MockPageDriver::new()),
            platform,
        };
        registry.register(session_id.to_string(), entry).await;
    }

    #[tokio::test]
    async fn submit_2fa_returns_not_found_when_session_missing() {
        let mut sessions = MockSessionRepository::new();
        sessions.expect_get().return_once(|_| Ok(None));
        let cookies = MockCookieRepository::new();
        let snapshot = MockSnapshotSink::new();
        let active = Arc::new(ActiveSessionRegistry::new());

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_uc(sessions, cookies, active, drivers, snapshot);
        let err = uc.execute("missing", "123456").await.unwrap_err();
        assert!(matches!(err, ImauthError::NotFound(_)));
    }

    #[tokio::test]
    async fn submit_2fa_connected_saves_cookies() {
        let stored = Session::new("s1".into(), "instagram".into());
        let mut sessions = MockSessionRepository::new();
        sessions.expect_get().return_once({
            let s = stored.clone();
            move |_| Ok(Some(s))
        });
        sessions.expect_update().returning(|_| Ok(()));

        let mut cookies = MockCookieRepository::new();
        cookies
            .expect_save()
            .withf(|p, _c| p == "instagram")
            .times(1)
            .returning(|_, _| Ok(()));

        let snapshot = MockSnapshotSink::new();
        let active = Arc::new(ActiveSessionRegistry::new());
        register_active_session(&active, "s1", Platform::Instagram).await;

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_uc(sessions, cookies, active.clone(), drivers, snapshot);
        let (out, cookies) = uc.execute("s1", "654321").await.unwrap();
        assert_eq!(out.state, SessionState::Connected);
        assert_eq!(cookies.len(), 1);
        // Binding must be released after submit succeeds so the browser
        // returns to the pool.
        assert_eq!(active.len().await, 0);
    }

    #[tokio::test]
    async fn submit_2fa_non_connected_skips_cookie_save() {
        let stored = Session::new("s1".into(), "instagram".into());
        let mut sessions = MockSessionRepository::new();
        sessions.expect_get().return_once({
            let s = stored.clone();
            move |_| Ok(Some(s))
        });
        sessions.expect_update().returning(|_| Ok(()));

        let mut cookies = MockCookieRepository::new();
        cookies.expect_save().times(0);

        let snapshot = MockSnapshotSink::new();
        let active = Arc::new(ActiveSessionRegistry::new());
        register_active_session(&active, "s1", Platform::Instagram).await;

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(FailingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_uc(sessions, cookies, active.clone(), drivers, snapshot);
        let (out, cookies) = uc.execute("s1", "000000").await.unwrap();
        assert_eq!(out.state, SessionState::Failed);
        assert!(cookies.is_empty());
        // Even on failure the binding is released — caller restarts via
        // a fresh Login flow.
        assert_eq!(active.len().await, 0);
    }

    #[tokio::test]
    async fn submit_2fa_returns_error_when_no_active_browser_binding() {
        // Session row exists but Login never registered a browser binding
        // (e.g., login already terminated or was cancelled). Returning a
        // typed Browser error makes the failure observable instead of
        // silently typing into a stranger's tab.
        let stored = Session::new("orphan".into(), "instagram".into());
        let mut sessions = MockSessionRepository::new();
        sessions.expect_get().return_once({
            let s = stored.clone();
            move |_| Ok(Some(s))
        });
        let cookies = MockCookieRepository::new();
        let snapshot = MockSnapshotSink::new();
        let active = Arc::new(ActiveSessionRegistry::new());

        let mut drivers = HashMap::new();
        drivers.insert(
            Platform::Instagram,
            Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>,
        );
        let uc = build_uc(sessions, cookies, active, drivers, snapshot);
        let err = uc.execute("orphan", "123456").await.unwrap_err();
        assert!(matches!(err, ImauthError::Browser(_)));
    }
}
