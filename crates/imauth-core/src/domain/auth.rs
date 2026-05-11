use crate::domain::platform::Platform;
use crate::domain::session::Cookie;

pub const CAPTCHA_PATTERNS: &[&str] = &[
    "verify your account",
    "suspicious login",
    "captcha",
    "security check",
];

pub const SUCCESS_INDICATORS: &[&str] = &[
    "save info",
    "save login info",
    "not now",
    "home",
    "feed",
    "reels",
    "search",
    "explore",
];

pub const FAILURE_INDICATORS: &[&str] = &[
    "incorrect",
    "wrong password",
    "couldn't sign you in",
    "invalid",
    "the login information you entered",
    "find your account",
    "username or password is incorrect",
    "password was incorrect",
];

pub const TWOFA_PATTERNS: &[&str] = &[
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

pub fn matches_any(lower_text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| lower_text.contains(p))
}

pub enum AuthCheckpoint {
    Captcha,
    Needs2Fa,
    Failed,
    Connected(Vec<Cookie>),
    Pending,
}

#[derive(Clone, Copy)]
pub enum Phase {
    Initial,
    PostLogin,
    Post2Fa,
}

impl Phase {
    pub fn failure_message(self) -> &'static str {
        match self {
            Phase::Initial | Phase::PostLogin => "Invalid credentials",
            Phase::Post2Fa => "Invalid 2FA code or credentials",
        }
    }

    pub fn captcha_message(self) -> &'static str {
        match self {
            Phase::PostLogin => "Captcha detected after login",
            _ => "Captcha detected",
        }
    }

    pub fn connected_message(self) -> &'static str {
        match self {
            Phase::Initial => "Already logged in",
            Phase::PostLogin => "Login successful",
            Phase::Post2Fa => "2FA verification successful",
        }
    }

    pub fn pending_message(self) -> &'static str {
        match self {
            Phase::Initial => "Login form did not appear in time",
            Phase::PostLogin => "Login did not complete. This can happen if credentials are wrong, \
                 the password is too short for Instagram's client-side check, or an extra security step is required. \
                 Try updating your credentials.",
            Phase::Post2Fa => "2FA verification did not complete",
        }
    }
}

/// Pure decision: given a page's visible text and its cookies, classify the auth state.
/// The adapter is responsible for fetching `page_text` and `cookies` from a real page.
pub fn classify_auth_state(
    page_text: &str,
    cookies: Vec<Cookie>,
    platform: Platform,
) -> AuthCheckpoint {
    let lower = page_text.to_lowercase();
    if matches_any(&lower, CAPTCHA_PATTERNS) {
        return AuthCheckpoint::Captcha;
    }
    if matches_any(&lower, FAILURE_INDICATORS) {
        return AuthCheckpoint::Failed;
    }
    if matches_any(&lower, TWOFA_PATTERNS) {
        return AuthCheckpoint::Needs2Fa;
    }

    let success_text = matches_any(&lower, SUCCESS_INDICATORS);
    let filtered = platform.filter_cookies(cookies);
    let session_cookie_name = platform.session_cookie_name();
    let has_session = filtered.iter().any(|c| c.name == session_cookie_name);

    if success_text || has_session {
        AuthCheckpoint::Connected(filtered)
    } else {
        AuthCheckpoint::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn cookie(name: &str, domain: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            expires: Some(Utc::now()),
            http_only: false,
            secure: true,
        }
    }

    fn check(checkpoint: &AuthCheckpoint) -> &'static str {
        match checkpoint {
            AuthCheckpoint::Captcha => "captcha",
            AuthCheckpoint::Needs2Fa => "2fa",
            AuthCheckpoint::Failed => "failed",
            AuthCheckpoint::Connected(_) => "connected",
            AuthCheckpoint::Pending => "pending",
        }
    }

    #[test]
    fn captcha_pattern_returns_captcha() {
        let cp = classify_auth_state(
            "Please complete the security check to continue",
            vec![],
            Platform::Instagram,
        );
        assert_eq!(check(&cp), "captcha");
    }

    #[test]
    fn failure_pattern_returns_failed() {
        let cp = classify_auth_state(
            "Sorry, your password was incorrect.",
            vec![],
            Platform::Instagram,
        );
        assert_eq!(check(&cp), "failed");
    }

    #[test]
    fn twofa_pattern_returns_needs_2fa() {
        let cp = classify_auth_state(
            "Enter the 6-digit code from your authenticator app",
            vec![],
            Platform::Instagram,
        );
        assert_eq!(check(&cp), "2fa");
    }

    #[test]
    fn success_text_plus_no_cookie_returns_connected() {
        let cp = classify_auth_state(
            "Welcome home — your feed is below",
            vec![],
            Platform::Instagram,
        );
        assert_eq!(check(&cp), "connected");
    }

    #[test]
    fn session_cookie_alone_returns_connected_with_filtered_cookies() {
        let cp = classify_auth_state(
            "nothing interesting here",
            vec![
                cookie("sessionid", ".instagram.com"),
                cookie("garbage", "example.com"),
            ],
            Platform::Instagram,
        );
        match cp {
            AuthCheckpoint::Connected(cookies) => {
                assert_eq!(cookies.len(), 1);
                assert_eq!(cookies[0].name, "sessionid");
            }
            _ => panic!("expected Connected"),
        }
    }

    #[test]
    fn neither_success_nor_cookie_returns_pending() {
        let cp = classify_auth_state(
            "boring page",
            vec![cookie("csrftoken", ".instagram.com")],
            Platform::Instagram,
        );
        assert_eq!(check(&cp), "pending");
    }

    #[test]
    fn captcha_pattern_wins_over_twofa_text() {
        let cp = classify_auth_state(
            "captcha required to verify your account before entering 2fa",
            vec![],
            Platform::Instagram,
        );
        assert_eq!(check(&cp), "captcha");
    }

    #[test]
    fn phase_messages_per_variant() {
        assert_eq!(Phase::Initial.failure_message(), "Invalid credentials");
        assert_eq!(
            Phase::Post2Fa.failure_message(),
            "Invalid 2FA code or credentials"
        );
        assert_eq!(Phase::Initial.connected_message(), "Already logged in");
        assert_eq!(Phase::PostLogin.connected_message(), "Login successful");
        assert_eq!(
            Phase::Post2Fa.connected_message(),
            "2FA verification successful"
        );
        assert_eq!(Phase::Initial.captcha_message(), "Captcha detected");
        assert_eq!(
            Phase::PostLogin.captcha_message(),
            "Captcha detected after login"
        );
        assert!(Phase::Initial.pending_message().contains("Login form"));
        assert!(Phase::Post2Fa.pending_message().contains("2FA"));
    }
}
