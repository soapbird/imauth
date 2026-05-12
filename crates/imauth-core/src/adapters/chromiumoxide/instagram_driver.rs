use crate::Result;
use crate::domain::auth::{classify_auth_state, AuthCheckpoint, Phase};
use crate::domain::selectors::INSTAGRAM_SELECTORS;
use crate::domain::session::{Session, SessionState};
use crate::domain::Platform;
use crate::ports::browser::{PageDriver, PlatformDriver};
use crate::ports::snapshot::SnapshotSink;
use crate::ImauthError;
use async_trait::async_trait;
use std::time::{Duration, Instant};

pub struct InstagramPlatformDriver;

impl InstagramPlatformDriver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for InstagramPlatformDriver {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_instagram_2fa_code(raw: &str) -> String {
    let trimmed = raw.trim();
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();

    match digits.len() {
        6 => digits,
        len if len > 6 => digits[len - 6..].to_string(),
        _ => trimmed.to_string(),
    }
}

async fn snapshot(page: &dyn PageDriver, sink: &dyn SnapshotSink, session_id: &str, label: &str) {
    let html = page.content_html().await.unwrap_or_default();
    let png = page.screenshot().await.ok();
    sink.capture(session_id, label, &html, png.as_deref()).await;
}

async fn inspect(page: &dyn PageDriver, platform: Platform) -> Result<AuthCheckpoint> {
    let (text, cookies) = tokio::try_join!(page.get_page_text(), page.get_cookies())?;
    Ok(classify_auth_state(&text, cookies, platform))
}

async fn wait_for_auth_checkpoint(
    page: &dyn PageDriver,
    platform: Platform,
    timeout: Duration,
    accept_2fa: bool,
) -> Result<AuthCheckpoint> {
    let deadline = Instant::now() + timeout;
    loop {
        let checkpoint = inspect(page, platform).await?;
        match checkpoint {
            AuthCheckpoint::Needs2Fa if !accept_2fa && Instant::now() < deadline => {}
            AuthCheckpoint::Pending if Instant::now() < deadline => {}
            other => return Ok(other),
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_login_form_or_checkpoint(
    page: &dyn PageDriver,
    platform: Platform,
    timeout: Duration,
) -> Result<Option<AuthCheckpoint>> {
    let deadline = Instant::now() + timeout;
    loop {
        if page
            .find_element(INSTAGRAM_SELECTORS.username_input)
            .await?
        {
            return Ok(None);
        }

        match inspect(page, platform).await? {
            AuthCheckpoint::Pending if Instant::now() < deadline => {}
            AuthCheckpoint::Pending => {
                return Err(ImauthError::Platform(format!(
                    "Timed out waiting for {}",
                    INSTAGRAM_SELECTORS.username_input
                )));
            }
            checkpoint => return Ok(Some(checkpoint)),
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn apply_checkpoint(
    page: &dyn PageDriver,
    sink: &dyn SnapshotSink,
    session: &mut Session,
    checkpoint: AuthCheckpoint,
    phase: Phase,
) -> Vec<crate::domain::session::Cookie> {
    match checkpoint {
        AuthCheckpoint::Connected(cookies) => {
            snapshot(page, sink, &session.id, "login_success").await;
            session.transition(
                SessionState::Connected,
                Some(phase.connected_message().to_string()),
            );
            cookies
        }
        AuthCheckpoint::Needs2Fa => {
            snapshot(page, sink, &session.id, "2fa_required").await;
            session.transition(SessionState::Needs2Fa, Some("2FA required".to_string()));
            vec![]
        }
        AuthCheckpoint::Captcha => {
            snapshot(page, sink, &session.id, "captcha_detected").await;
            session.transition(
                SessionState::NeedsCaptcha,
                Some(phase.captcha_message().to_string()),
            );
            vec![]
        }
        AuthCheckpoint::Failed => {
            snapshot(page, sink, &session.id, "login_failed").await;
            session.transition(
                SessionState::Failed,
                Some(phase.failure_message().to_string()),
            );
            vec![]
        }
        AuthCheckpoint::Pending => {
            snapshot(page, sink, &session.id, "login_failed").await;
            session.transition(
                SessionState::Failed,
                Some(phase.pending_message().to_string()),
            );
            vec![]
        }
    }
}

#[async_trait]
impl PlatformDriver for InstagramPlatformDriver {
    async fn login<'a>(
        &'a self,
        page: &'a dyn PageDriver,
        platform: Platform,
        username: &'a str,
        password: &'a str,
        session: &'a mut Session,
        sink: &'a dyn SnapshotSink,
    ) -> Result<Vec<crate::domain::session::Cookie>> {
        session.transition(
            SessionState::Loading,
            Some(format!("Opening {} login page...", platform.as_str())),
        );

        page.navigate("https://www.instagram.com/accounts/login/", 30)
            .await?;

        let initial_checkpoint =
            wait_for_login_form_or_checkpoint(page, platform, Duration::from_secs(10)).await?;

        snapshot(page, sink, &session.id, "loading_page").await;

        if let Some(checkpoint) = initial_checkpoint {
            let cookies = apply_checkpoint(page, sink, session, checkpoint, Phase::Initial).await;
            return Ok(cookies);
        }

        session.transition(
            SessionState::Authenticating,
            Some("Filling credentials...".to_string()),
        );

        page.fill_input(INSTAGRAM_SELECTORS.username_input, username)
            .await
            .map_err(|e| ImauthError::Platform(format!("Could not find username field: {e}")))?;
        page.fill_input(INSTAGRAM_SELECTORS.password_input, password)
            .await
            .map_err(|e| ImauthError::Platform(format!("Could not find password field: {e}")))?;

        snapshot(page, sink, &session.id, "creds_filled").await;

        if let Err(click_err) = page.click_element(INSTAGRAM_SELECTORS.submit_button).await {
            tracing::warn!("Failed to click submit button; falling back to Enter: {click_err}");
            page.press_enter(INSTAGRAM_SELECTORS.password_input)
                .await
                .map_err(|e| ImauthError::Platform(format!("Failed to submit form: {e}")))?;
        }

        let checkpoint =
            wait_for_auth_checkpoint(page, platform, Duration::from_secs(15), true).await?;

        snapshot(page, sink, &session.id, "after_submit").await;

        let cookies = apply_checkpoint(page, sink, session, checkpoint, Phase::PostLogin).await;

        Ok(cookies)
    }

    async fn submit_2fa<'a>(
        &'a self,
        page: &'a dyn PageDriver,
        platform: Platform,
        code: &'a str,
        session: &'a mut Session,
        sink: &'a dyn SnapshotSink,
    ) -> Result<Vec<crate::domain::session::Cookie>> {
        session.transition(
            SessionState::Authenticating,
            Some("Submitting 2FA code...".to_string()),
        );

        let mut input_selector: Option<&'static str> = None;
        let code = normalize_instagram_2fa_code(code);
        for selector in INSTAGRAM_SELECTORS.twofa_input.iter().copied() {
            if page.find_element(selector).await? {
                page.fill_input(selector, &code).await?;
                input_selector = Some(selector);
                break;
            }
        }

        let Some(input_selector) = input_selector else {
            return Err(ImauthError::Platform(
                "Could not find 2FA code input".to_string(),
            ));
        };

        let texts = vec![
            "continue".to_string(),
            "submit".to_string(),
            "log in".to_string(),
        ];
        let mut submitted = page
            .click_element_text("button, input[type='submit'], div[role='button']", &texts)
            .await?;

        if !submitted {
            for submit_sel in INSTAGRAM_SELECTORS.twofa_submit.iter().copied() {
                if page.find_element(submit_sel).await?
                    && page.click_element(submit_sel).await.is_ok()
                {
                    submitted = true;
                    break;
                }
            }
        }

        if !submitted {
            page.press_enter(input_selector).await?;
        }

        snapshot(page, sink, &session.id, "2fa_submitting").await;

        let checkpoint =
            wait_for_auth_checkpoint(page, platform, Duration::from_secs(15), false).await?;

        snapshot(page, sink, &session.id, "2fa_after_submit").await;

        let cookies = apply_checkpoint(page, sink, session, checkpoint, Phase::Post2Fa).await;

        Ok(cookies)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_instagram_2fa_code;

    #[test]
    fn normalize_instagram_2fa_code_preserves_leading_zeroes() {
        assert_eq!(normalize_instagram_2fa_code("007594"), "007594");
    }

    #[test]
    fn normalize_instagram_2fa_code_drops_terminal_echo_prefix() {
        assert_eq!(normalize_instagram_2fa_code("746736362"), "736362");
    }

    #[test]
    fn normalize_instagram_2fa_code_strips_digit_separators() {
        assert_eq!(normalize_instagram_2fa_code("736 362"), "736362");
    }
}
