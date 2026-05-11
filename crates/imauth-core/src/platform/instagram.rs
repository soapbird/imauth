use crate::browser::cdp::{fill_input, get_cookies, get_page_text, navigate, press_enter};
use crate::platform::selectors::INSTAGRAM_SELECTORS;
use crate::session::state::{Cookie, Session, SessionState};
use crate::ImauthError;
use chromiumoxide::page::Page;
use std::time::Duration;

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

fn detect_captcha(text: &str) -> bool {
    let lower = text.to_lowercase();
    CAPTCHA_PATTERNS.iter().any(|p| lower.contains(p))
}

fn detect_2fa(text: &str) -> bool {
    let lower = text.to_lowercase();
    TW0FA_PATTERNS.iter().any(|p| lower.contains(p))
}

fn detect_success(text: &str) -> bool {
    let lower = text.to_lowercase();
    SUCCESS_INDICATORS.iter().any(|p| lower.contains(p))
}

fn detect_failure(text: &str) -> bool {
    let lower = text.to_lowercase();
    FAILURE_INDICATORS.iter().any(|p| lower.contains(p))
}

pub async fn login(
    page: &Page,
    username: &str,
    password: &str,
    session: &mut Session,
) -> crate::Result<()> {
    session.transition(SessionState::Loading, Some("Opening Instagram login page...".to_string()));

    navigate(
        page,
        "https://www.instagram.com/accounts/login/",
        30,
    )
    .await?;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let page_text = get_page_text(page).await?;
    if detect_captcha(&page_text) {
        session.transition(SessionState::NeedsCaptcha, Some("Captcha detected".to_string()));
        return Ok(());
    }

    session.transition(SessionState::Authenticating, Some("Filling credentials...".to_string()));

    // Use email/pass selectors as discovered in auto-auth.py
    fill_input(page, INSTAGRAM_SELECTORS.username_input, username)
        .await
        .map_err(|e| ImauthError::Platform(format!("Could not find username field: {e}")))?;
    fill_input(page, INSTAGRAM_SELECTORS.password_input, password)
        .await
        .map_err(|e| ImauthError::Platform(format!("Could not find password field: {e}")))?;
    press_enter(page, INSTAGRAM_SELECTORS.password_input)
        .await
        .map_err(|e| ImauthError::Platform(format!("Failed to submit form: {e}")))?;

    // Handle post-login flow (8 second wait for server-side verification)
    tokio::time::sleep(Duration::from_secs(8)).await;

    let page_text = get_page_text(page).await?;

    if detect_captcha(&page_text) {
        session.transition(SessionState::NeedsCaptcha, Some("Captcha detected after login".to_string()));
        return Ok(());
    }

    if detect_2fa(&page_text) {
        session.transition(SessionState::Needs2Fa, Some("2FA required".to_string()));
        return Ok(());
    }

    if detect_failure(&page_text) {
        session.transition(SessionState::Failed, Some("Invalid credentials".to_string()));
        return Ok(());
    }

    // Collect cookies - use sessionid as ground-truth signal
    let cookies = get_cookies(page).await?;
    let filtered: Vec<Cookie> = cookies
        .into_iter()
        .filter(|c| {
            let d = c.domain.to_lowercase().trim_start_matches('.').to_string();
            d == "instagram.com" || d.ends_with(".instagram.com")
        })
        .collect();

    let has_session_cookie = filtered.iter().any(|c| c.name == "sessionid");
    let success_detected = detect_success(&page_text);

    if success_detected || has_session_cookie {
        session.cookies = filtered;
        session.transition(SessionState::Connected, Some("Login successful".to_string()));
    } else {
        session.transition(
            SessionState::Failed,
            Some(
                "Login did not complete. This can happen if credentials are wrong, \
                 the password is too short for Instagram's client-side check, or an extra security step is required. \
                 Try updating your credentials."
                    .to_string(),
            ),
        );
    }

    Ok(())
}

pub async fn submit_2fa(
    page: &Page,
    code: &str,
    session: &mut Session,
) -> crate::Result<()> {
    session.transition(SessionState::Authenticating, Some("Submitting 2FA code...".to_string()));

    // Find and fill 2FA input
    for selector in INSTAGRAM_SELECTORS.twofa_input.iter().copied() {
        if page.find_element(selector).await.is_ok() {
            fill_input(page, selector, code).await?;

            // Try to find submit button
            for submit_sel in INSTAGRAM_SELECTORS.twofa_submit.iter().copied() {
                if page.find_element(submit_sel).await.is_ok() {
                    page.find_element(submit_sel)
                        .await
                        .map_err(|e| ImauthError::Platform(format!("2FA submit button not found: {e}")))?
                        .click()
                        .await
                        .map_err(|e| ImauthError::Platform(format!("Failed to click 2FA submit: {e}")))?;
                    break;
                }
            }

            // Press Enter as fallback
            press_enter(page, selector).await?;
            break;
        }
    }

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Handle post-login flow again
    tokio::time::sleep(Duration::from_secs(8)).await;

    let page_text = get_page_text(page).await?;

    if detect_failure(&page_text) {
        session.transition(SessionState::Failed, Some("Invalid 2FA code or credentials".to_string()));
        return Ok(());
    }

    let cookies = get_cookies(page).await?;
    let filtered: Vec<Cookie> = cookies
        .into_iter()
        .filter(|c| {
            let binding = c.domain.to_lowercase();
            let d = binding.trim_start_matches('.');
            d == "instagram.com" || d.ends_with(".instagram.com")
        })
        .collect();

    let has_session_cookie = filtered.iter().any(|c| c.name == "sessionid");
    let success_detected = detect_success(&page_text);

    if success_detected || has_session_cookie {
        session.cookies = filtered;
        session.transition(SessionState::Connected, Some("2FA verification successful".to_string()));
    } else {
        session.transition(SessionState::Failed, Some("2FA verification did not complete".to_string()));
    }

    Ok(())
}
