use crate::domain::platform::Platform;
use crate::domain::session::Cookie;

pub enum AuthCheckpoint {
    Connected(Vec<Cookie>),
    Pending,
}

/// Pure decision: given a page's cookies, check if the user has logged in.
/// Login is now user-driven — we only detect session cookie presence.
pub fn classify_auth_state(cookies: Vec<Cookie>, platform: Platform) -> AuthCheckpoint {
    if platform.has_session_cookie(&cookies) {
        AuthCheckpoint::Connected(platform.filter_cookies(cookies))
    } else {
        AuthCheckpoint::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn cookie(name: &str, domain: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            expires: Some(Utc::now() + Duration::hours(1)),
            http_only: false,
            secure: true,
        }
    }

    #[test]
    fn session_cookie_returns_connected_with_filtered_cookies() {
        let cp = classify_auth_state(
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
    fn no_session_cookie_returns_pending() {
        let cp = classify_auth_state(
            vec![cookie("csrftoken", ".instagram.com")],
            Platform::Instagram,
        );
        assert!(matches!(cp, AuthCheckpoint::Pending));
    }

    #[test]
    fn naver_session_cookie_returns_connected() {
        let cp = classify_auth_state(vec![cookie("NID_AUT", ".naver.com")], Platform::Naver);
        assert!(matches!(cp, AuthCheckpoint::Connected(_)));
    }

    #[test]
    fn naver_wrong_domain_returns_pending() {
        let cp = classify_auth_state(vec![cookie("NID_AUT", ".example.com")], Platform::Naver);
        assert!(matches!(cp, AuthCheckpoint::Pending));
    }
}
