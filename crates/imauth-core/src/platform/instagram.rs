use crate::browser::cdp::{
    click_element, click_element_text, fill_input, get_cookies, get_page_text, navigate,
    press_enter,
};
use crate::platform::selectors::INSTAGRAM_SELECTORS;
use crate::platform::Platform;
use crate::session::state::{Cookie, Session, SessionState};
use crate::snapshot::SnapshotCtx;
use crate::ImauthError;
use chromiumoxide::page::Page;
use std::path::Path;
use std::time::{Duration, Instant};

const CAPTCHA_PATTERNS: &[&str] = &[
    "verify your account",
    "suspicious login",
    "captcha",
    "security check",
];

const SUCCESS_INDICATORS: &[&str] = &[
    "save info",
    "save login info",
    "not now",
    "home",
    "feed",
    "reels",
    "search",
    "explore",
];

const FAILURE_INDICATORS: &[&str] = &[
    "incorrect",
    "wrong password",
    "couldn't sign you in",
    "invalid",
    "the login information you entered",
    "find your account",
    "username or password is incorrect",
    "password was incorrect",
];

const TW0FA_PATTERNS: &[&str] = &[
    "two-factor",
    "two factor",
    "2fa",
    "authentication code",
    "verification code",
    "6-digit",
    "6 digit",
    "enter code",
    "security code",
    "2-step",
    "2step",
    "two-step",
];

fn matches_any(lower_text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| lower_text.contains(p))
}

enum AuthCheckpoint {
    Captcha,
    Needs2Fa,
    Failed,
    Connected(Vec<Cookie>),
    Pending,
}

async fn inspect_auth_checkpoint(page: &Page, platform: Platform) -> crate::Result<AuthCheckpoint> {
    let page_text = get_page_text(page).await?.to_lowercase();

    if matches_any(&page_text, CAPTCHA_PATTERNS) {
        return Ok(AuthCheckpoint::Captcha);
    }
    if matches_any(&page_text, FAILURE_INDICATORS) {
        return Ok(AuthCheckpoint::Failed);
    }
    if matches_any(&page_text, TW0FA_PATTERNS) {
        return Ok(AuthCheckpoint::Needs2Fa);
    }

    let success_text = matches_any(&page_text, SUCCESS_INDICATORS);
    let filtered = platform.filter_cookies(get_cookies(page).await?);

    if success_text || platform.has_session_cookie(&filtered) {
        Ok(AuthCheckpoint::Connected(filtered))
    } else {
        Ok(AuthCheckpoint::Pending)
    }
}

async fn wait_for_auth_checkpoint(
    page: &Page,
    platform: Platform,
    timeout: Duration,
    accept_2fa: bool,
) -> crate::Result<AuthCheckpoint> {
    let deadline = Instant::now() + timeout;
    loop {
        let checkpoint = inspect_auth_checkpoint(page, platform).await?;
        match checkpoint {
            AuthCheckpoint::Needs2Fa if !accept_2fa && Instant::now() < deadline => {}
            AuthCheckpoint::Pending if Instant::now() < deadline => {}
            other => return Ok(other),
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_login_form_or_checkpoint(
    page: &Page,
    platform: Platform,
    timeout: Duration,
) -> crate::Result<Option<AuthCheckpoint>> {
    let deadline = Instant::now() + timeout;
    loop {
        if page
            .find_element(INSTAGRAM_SELECTORS.username_input)
            .await
            .is_ok()
        {
            return Ok(None);
        }

        match inspect_auth_checkpoint(page, platform).await? {
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

#[derive(Clone, Copy)]
enum Phase {
    Initial,
    PostLogin,
    Post2Fa,
}

impl Phase {
    fn failure_message(self) -> &'static str {
        match self {
            Phase::Initial | Phase::PostLogin => "Invalid credentials",
            Phase::Post2Fa => "Invalid 2FA code or credentials",
        }
    }

    fn captcha_message(self) -> &'static str {
        match self {
            Phase::PostLogin => "Captcha detected after login",
            _ => "Captcha detected",
        }
    }

    fn connected_message(self) -> &'static str {
        match self {
            Phase::Initial => "Already logged in",
            Phase::PostLogin => "Login successful",
            Phase::Post2Fa => "2FA verification successful",
        }
    }

    fn pending_message(self) -> &'static str {
        match self {
            Phase::Initial => "Login form did not appear in time",
            Phase::PostLogin => "Login did not complete. This can happen if credentials are wrong, \
                 the password is too short for Instagram's client-side check, or an extra security step is required. \
                 Try updating your credentials.",
            Phase::Post2Fa => "2FA verification did not complete",
        }
    }
}

async fn apply_checkpoint(
    page: &Page,
    snap: &mut SnapshotCtx<'_>,
    session: &mut Session,
    checkpoint: AuthCheckpoint,
    phase: Phase,
) {
    match checkpoint {
        AuthCheckpoint::Connected(cookies) => {
            session.cookies = cookies;
            snap.capture(page, "login_success").await;
            session.transition(
                SessionState::Connected,
                Some(phase.connected_message().to_string()),
            );
        }
        AuthCheckpoint::Needs2Fa => {
            snap.capture(page, "2fa_required").await;
            session.transition(SessionState::Needs2Fa, Some("2FA required".to_string()));
        }
        AuthCheckpoint::Captcha => {
            snap.capture(page, "captcha_detected").await;
            session.transition(
                SessionState::NeedsCaptcha,
                Some(phase.captcha_message().to_string()),
            );
        }
        AuthCheckpoint::Failed => {
            snap.capture(page, "login_failed").await;
            session.transition(
                SessionState::Failed,
                Some(phase.failure_message().to_string()),
            );
        }
        AuthCheckpoint::Pending => {
            snap.capture(page, "login_failed").await;
            session.transition(
                SessionState::Failed,
                Some(phase.pending_message().to_string()),
            );
        }
    }
}

pub async fn login(
    page: &Page,
    platform: Platform,
    username: &str,
    password: &str,
    session: &mut Session,
    snapshot_dir: Option<&Path>,
) -> crate::Result<()> {
    session.transition(
        SessionState::Loading,
        Some(format!("Opening {} login page...", platform.as_str())),
    );

    let session_dir = snapshot_dir.map(|d| d.join(&session.id));
    let mut snap = SnapshotCtx::new(session_dir.as_deref());

    navigate(page, "https://www.instagram.com/accounts/login/", 30).await?;

    let initial_checkpoint =
        wait_for_login_form_or_checkpoint(page, platform, Duration::from_secs(10)).await?;

    snap.capture(page, "loading_page").await;

    if let Some(checkpoint) = initial_checkpoint {
        apply_checkpoint(page, &mut snap, session, checkpoint, Phase::Initial).await;
        return Ok(());
    }

    session.transition(
        SessionState::Authenticating,
        Some("Filling credentials...".to_string()),
    );

    fill_input(page, INSTAGRAM_SELECTORS.username_input, username)
        .await
        .map_err(|e| ImauthError::Platform(format!("Could not find username field: {e}")))?;
    fill_input(page, INSTAGRAM_SELECTORS.password_input, password)
        .await
        .map_err(|e| ImauthError::Platform(format!("Could not find password field: {e}")))?;

    snap.capture(page, "creds_filled").await;

    if let Err(click_err) = click_element(page, INSTAGRAM_SELECTORS.submit_button).await {
        tracing::warn!("Failed to click submit button; falling back to Enter: {click_err}");
        press_enter(page, INSTAGRAM_SELECTORS.password_input)
            .await
            .map_err(|e| ImauthError::Platform(format!("Failed to submit form: {e}")))?;
    }

    let checkpoint = wait_for_auth_checkpoint(page, platform, Duration::from_secs(15), true).await?;

    snap.capture(page, "after_submit").await;

    apply_checkpoint(page, &mut snap, session, checkpoint, Phase::PostLogin).await;

    Ok(())
}

pub async fn submit_2fa(
    page: &Page,
    platform: Platform,
    code: &str,
    session: &mut Session,
    snapshot_dir: Option<&Path>,
) -> crate::Result<()> {
    session.transition(
        SessionState::Authenticating,
        Some("Submitting 2FA code...".to_string()),
    );

    let session_dir = snapshot_dir.map(|d| d.join(&session.id));
    let mut snap = SnapshotCtx::new(session_dir.as_deref());

    let mut input_selector = None;

    for selector in INSTAGRAM_SELECTORS.twofa_input.iter().copied() {
        if page.find_element(selector).await.is_ok() {
            fill_input(page, selector, code).await?;
            input_selector = Some(selector);
            break;
        }
    }

    let Some(input_selector) = input_selector else {
        return Err(ImauthError::Platform(
            "Could not find 2FA code input".to_string(),
        ));
    };

    let mut submitted = click_element_text(
        page,
        "button, input[type='submit'], div[role='button']",
        &["continue", "submit", "log in"],
    )
    .await?;

    if !submitted {
        for submit_sel in INSTAGRAM_SELECTORS.twofa_submit.iter().copied() {
            if page.find_element(submit_sel).await.is_ok()
                && click_element(page, submit_sel).await.is_ok()
            {
                submitted = true;
                break;
            }
        }
    }

    if !submitted {
        press_enter(page, input_selector).await?;
    }

    snap.capture(page, "2fa_submitting").await;

    let checkpoint =
        wait_for_auth_checkpoint(page, platform, Duration::from_secs(15), false).await?;

    snap.capture(page, "2fa_after_submit").await;

    apply_checkpoint(page, &mut snap, session, checkpoint, Phase::Post2Fa).await;

    Ok(())
}
