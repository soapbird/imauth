use crate::Result;
use crate::domain::session::{Cookie, Session, SessionState};
use crate::domain::Platform;
use crate::ports::browser::{BrowserSessionFactory, PlatformDriver};
use crate::ports::repository::{CookieRepository, SessionRepository};
use crate::ports::snapshot::SnapshotSink;
use crate::ImauthError;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Submit2FaUseCase {
    sessions: Arc<dyn SessionRepository>,
    cookies: Arc<dyn CookieRepository>,
    browser: Arc<dyn BrowserSessionFactory>,
    drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
    snapshot: Arc<dyn SnapshotSink>,
}

impl Submit2FaUseCase {
    pub fn new(
        sessions: Arc<dyn SessionRepository>,
        cookies: Arc<dyn CookieRepository>,
        browser: Arc<dyn BrowserSessionFactory>,
        drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
        snapshot: Arc<dyn SnapshotSink>,
    ) -> Self {
        Self {
            sessions,
            cookies,
            browser,
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

        let browser_session = self.browser.acquire().await?;
        let pages = browser_session.existing_pages().await?;
        let page = pages.into_iter().next().ok_or_else(|| {
            ImauthError::Browser("No browser page found — the 2FA page may have been closed".into())
        })?;

        let cookies = driver
            .submit_2fa(&*page, platform, code, &mut session, &*self.snapshot)
            .await?;

        self.sessions.update(&session).await?;

        if session.state == SessionState::Connected {
            self.cookies
                .save(&session.platform, &cookies)
                .await?;
        }

        Ok((session, cookies))
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
        browser: MockBrowserSessionFactory,
        drivers: HashMap<Platform, Arc<dyn PlatformDriver>>,
        snapshot: MockSnapshotSink,
    ) -> Submit2FaUseCase {
        Submit2FaUseCase::new(
            Arc::new(sessions),
            Arc::new(cookies),
            Arc::new(browser),
            drivers,
            Arc::new(snapshot),
        )
    }

    fn happy_browser() -> MockBrowserSessionFactory {
        let mut factory = MockBrowserSessionFactory::new();
        factory.expect_acquire().return_once(|| {
            let mut bs = MockBrowserSession::new();
            bs.expect_existing_pages().return_once(|| {
                let page = MockPageDriver::new();
                Ok(vec![
                    Box::new(page) as Box<dyn crate::ports::browser::PageDriver>
                ])
            });
            Ok(Box::new(bs))
        });
        factory
    }

    #[tokio::test]
    async fn submit_2fa_returns_not_found_when_session_missing() {
        let mut sessions = MockSessionRepository::new();
        sessions.expect_get().return_once(|_| Ok(None));
        let cookies = MockCookieRepository::new();
        let browser = MockBrowserSessionFactory::new();
        let snapshot = MockSnapshotSink::new();

        let mut drivers = HashMap::new();
        drivers.insert(Platform::Instagram, Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>);
        let uc = build_uc(
            sessions,
            cookies,
            browser,
            drivers,
            snapshot,
        );
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
        let mut drivers = HashMap::new();
        drivers.insert(Platform::Instagram, Arc::new(ConnectingDriver) as Arc<dyn PlatformDriver>);
        let uc = build_uc(
            sessions,
            cookies,
            happy_browser(),
            drivers,
            snapshot,
        );
        let (out, cookies) = uc.execute("s1", "654321").await.unwrap();
        assert_eq!(out.state, SessionState::Connected);
        assert_eq!(cookies.len(), 1);
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
        let mut drivers = HashMap::new();
        drivers.insert(Platform::Instagram, Arc::new(FailingDriver) as Arc<dyn PlatformDriver>);
        let uc = build_uc(
            sessions,
            cookies,
            happy_browser(),
            drivers,
            snapshot,
        );
        let (out, cookies) = uc.execute("s1", "000000").await.unwrap();
        assert_eq!(out.state, SessionState::Failed);
        assert!(cookies.is_empty());
    }
}
