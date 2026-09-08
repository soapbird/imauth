mod control;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod tests;

use self::control::{LoginControl, LoginFailure, CLEANUP_TIMEOUT};
use crate::domain::auth::{classify_auth_state, AuthCheckpoint};
use crate::domain::session::{Cookie, Session, SessionState};
use crate::domain::Platform;
use crate::ports::browser::{BrowserSession, BrowserSessionFactory};
use crate::ports::repository::{CookieRepository, SessionRepository};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

const DEFAULT_PREPARATION_TIMEOUT: Duration = Duration::from_secs(90);
const DEFAULT_PAGE_TIMEOUT: Duration = Duration::from_secs(30);
const COOKIE_POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_ERRORS_BEFORE_RECONNECT: u32 = 3;

#[derive(Debug, Clone)]
pub enum LoginEvent {
    Started(Session),
    WaitingForUser(Session, String),
    Final(Session, Vec<Cookie>),
}

pub struct LoginUseCase {
    sessions: Arc<dyn SessionRepository>,
    cookies: Arc<dyn CookieRepository>,
    browser: Arc<dyn BrowserSessionFactory>,
    login_timeout: Duration,
    preparation_timeout: Duration,
    page_timeout: Duration,
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
            preparation_timeout: DEFAULT_PREPARATION_TIMEOUT,
            page_timeout: DEFAULT_PAGE_TIMEOUT,
        }
    }

    pub fn with_browser_timeouts(mut self, preparation: Duration, page: Duration) -> Self {
        self.preparation_timeout = preparation;
        self.page_timeout = page;
        self
    }

    pub async fn execute(&self, platform: Platform, tx: mpsc::Sender<LoginEvent>) {
        let preparation_deadline = Instant::now() + self.preparation_timeout;
        let initial = Session::new(uuid::Uuid::new_v4().to_string(), platform.as_str().into());
        let created = tokio::select! {
            biased;
            _ = tx.closed() => return,
            result = tokio::time::timeout(CLEANUP_TIMEOUT, self.sessions.create(initial.clone())) => result,
        };
        let mut session = match created {
            Ok(Ok(session)) => session,
            error => {
                tracing::error!(?error, "failed to create login session");
                let mut failed = initial;
                failed.transition(
                    SessionState::Failed,
                    Some("Failed to create session".into()),
                );
                let _ = tokio::time::timeout(
                    CLEANUP_TIMEOUT,
                    tx.send(LoginEvent::Final(failed, vec![])),
                )
                .await;
                return;
            }
        };
        let session_id = session.id.clone();
        let control = LoginControl {
            sessions: self.sessions.as_ref(),
            session_id: &session_id,
            tx: &tx,
            deadline: preparation_deadline,
            stage: "browser preparation",
        };
        let result = async {
            control
                .run(send(&tx, LoginEvent::Started(session.clone())))
                .await?;
            let mut browser = control.run(self.browser.acquire()).await?;
            let result = self
                .login_in_browser(platform, &mut session, browser.as_mut(), &control)
                .await;
            match tokio::time::timeout(CLEANUP_TIMEOUT, browser.close()).await {
                Ok(Ok(())) => {}
                error => tracing::warn!(%session_id, ?error, "failed to close login browser"),
            }
            result
        }
        .await;

        let cookies = match result {
            Ok(cookies) => cookies,
            Err(error) => {
                session.transition(SessionState::Failed, Some(error.to_string()));
                if let Err(error) = tokio::time::timeout(CLEANUP_TIMEOUT, async {
                    self.sessions.update(&session).await
                })
                .await
                .unwrap_or_else(|_| {
                    Err(crate::ImauthError::Database(
                        "Session update timed out".into(),
                    ))
                }) {
                    tracing::warn!(%session_id, %error, "failed to persist terminal login state");
                }
                vec![]
            }
        };
        let _ = tokio::time::timeout(
            CLEANUP_TIMEOUT,
            tx.send(LoginEvent::Final(session, cookies)),
        )
        .await;
    }

    async fn login_in_browser(
        &self,
        platform: Platform,
        session: &mut Session,
        browser: &mut dyn BrowserSession,
        preparation: &LoginControl<'_>,
    ) -> Result<Vec<Cookie>, LoginFailure> {
        let page_control = LoginControl {
            deadline: preparation.deadline.min(Instant::now() + self.page_timeout),
            stage: "page creation",
            ..*preparation
        };
        let mut page = page_control.run(browser.new_page()).await?;
        session.transition(
            SessionState::Loading,
            Some(format!("Opening {} login page...", platform.as_str())),
        );
        preparation.run(self.sessions.update(session)).await?;
        preparation
            .run(page.navigate(platform.login_url(), self.page_timeout.as_secs()))
            .await?;
        session.transition(
            SessionState::WaitingForUser,
            Some("Waiting for user to log in via browser".into()),
        );
        preparation.run(self.sessions.update(session)).await?;
        preparation
            .run(send(
                preparation.tx,
                LoginEvent::WaitingForUser(session.clone(), browser.viewer_url()),
            ))
            .await?;

        let control = LoginControl {
            deadline: Instant::now() + self.login_timeout,
            stage: "user login",
            ..*preparation
        };
        let mut errors = 0;
        loop {
            if control.run(self.sessions.get(&session.id)).await?.is_none() {
                return Err(LoginFailure::Cancelled);
            }
            let result = control.run(async {
                tokio::select! {
                    biased;
                    cookies = page.get_cookies() => cookies,
                    _ = browser.wait_disconnected() => Err(crate::ImauthError::Browser("CDP disconnected".into())),
                }
            }).await;
            match result {
                Ok(raw) => {
                    errors = 0;
                    if let AuthCheckpoint::Connected(cookies) = classify_auth_state(raw, platform) {
                        session
                            .transition(SessionState::Connected, Some("Login successful".into()));
                        // Persist outside the user-login deadline: cancelling an
                        // in-flight SQLite commit can store cookies under a
                        // session the error path then marks failed.
                        match tokio::time::timeout(
                            CLEANUP_TIMEOUT,
                            self.cookies.save_login(session, &cookies),
                        )
                        .await
                        {
                            Ok(result) => result.map_err(LoginFailure::from)?,
                            Err(_) => {
                                let stored = tokio::time::timeout(
                                    CLEANUP_TIMEOUT,
                                    self.sessions.get(&session.id),
                                )
                                .await;
                                match stored {
                                    Ok(Ok(Some(stored)))
                                        if stored.state == SessionState::Connected => {}
                                    _ => {
                                        return Err(LoginFailure::Operation(
                                            crate::ImauthError::Database(
                                                "login persistence timed out".into(),
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                        return Ok(cookies);
                    }
                }
                Err(LoginFailure::Operation(error)) => {
                    errors += 1;
                    tracing::warn!(session_id = %session.id, %error, errors, "cookie poll failed");
                    if errors >= MAX_ERRORS_BEFORE_RECONNECT {
                        page = control.run(browser.reconnect()).await?;
                        errors = 0;
                        continue;
                    }
                }
                Err(error) => return Err(error),
            }
            let disconnected = control
                .run(async {
                    tokio::select! {
                        _ = browser.wait_disconnected() => Ok(true),
                        _ = tokio::time::sleep(COOKIE_POLL_INTERVAL) => Ok(false),
                    }
                })
                .await?;
            if disconnected {
                page = control.run(browser.reconnect()).await?;
                errors = 0;
            }
        }
    }
}

async fn send(tx: &mpsc::Sender<LoginEvent>, event: LoginEvent) -> crate::Result<()> {
    tx.send(event)
        .await
        .map_err(|_| crate::ImauthError::Browser("Login cancelled".into()))
}
