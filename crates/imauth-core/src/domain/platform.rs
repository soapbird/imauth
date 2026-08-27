use crate::domain::session::Cookie;
use chrono::Utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Instagram,
    Threads,
    Naver,
    Novelpia,
    Munpia,
}

impl Platform {
    pub const ALL: &'static [Platform] = &[
        Platform::Instagram,
        Platform::Threads,
        Platform::Naver,
        Platform::Novelpia,
        Platform::Munpia,
    ];

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "instagram" => Some(Platform::Instagram),
            "threads" => Some(Platform::Threads),
            "naver" => Some(Platform::Naver),
            "novelpia" => Some(Platform::Novelpia),
            "munpia" => Some(Platform::Munpia),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Instagram => "instagram",
            Platform::Threads => "threads",
            Platform::Naver => "naver",
            Platform::Novelpia => "novelpia",
            Platform::Munpia => "munpia",
        }
    }

    pub fn login_url(&self) -> &'static str {
        match self {
            Platform::Instagram | Platform::Threads => "https://www.instagram.com/accounts/login/",
            Platform::Naver => "https://nid.naver.com/nidlogin.login",
            Platform::Novelpia => "https://novelpia.com/login/",
            Platform::Munpia => "https://nssl.munpia.com/login",
        }
    }

    pub fn cookie_domains(&self) -> &'static [&'static str] {
        match self {
            Platform::Instagram | Platform::Threads => {
                &[".instagram.com", ".threads.net", ".threads.com"]
            }
            Platform::Naver => &[".naver.com", ".nid.naver.com"],
            Platform::Novelpia => &[".novelpia.com"],
            Platform::Munpia => &[".munpia.com"],
        }
    }

    pub fn session_cookie_name(&self) -> &'static str {
        match self {
            Platform::Instagram | Platform::Threads => "sessionid",
            Platform::Naver => "NID_AUT",
            Platform::Novelpia => "AUTOLOGIN",
            Platform::Munpia => "TOKEN",
        }
    }

    fn cookie_matches_domain(&self, cookie: &Cookie) -> bool {
        let cookie_domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
        self.cookie_domains().iter().any(|allow| {
            let allow = allow.trim_start_matches('.').to_ascii_lowercase();
            cookie_domain == allow || cookie_domain.ends_with(&format!(".{allow}"))
        })
    }

    pub fn filter_cookies(&self, cookies: Vec<Cookie>) -> Vec<Cookie> {
        cookies
            .into_iter()
            .filter(|c| self.cookie_matches_domain(c))
            .collect()
    }

    pub fn session_cookie<'a>(&self, cookies: &'a [Cookie]) -> Option<&'a Cookie> {
        if *self == Platform::Novelpia {
            let login_key = self.active_cookie(cookies, "LOGINKEY")?;
            if login_key.value.is_empty()
                || self
                    .active_cookie(cookies, "USERKEY")
                    .is_none_or(|cookie| cookie.value.is_empty())
            {
                return None;
            }

            return self
                .active_cookie(cookies, "AUTOLOGIN")
                .filter(|cookie| !cookie.value.is_empty())
                .or_else(|| {
                    self.active_cookie(cookies, "ISLOGIN")
                        .filter(|cookie| cookie.value == "1")
                });
        }

        let name = self.session_cookie_name();
        self.active_cookie(cookies, name)
    }

    pub fn has_session_cookie(&self, cookies: &[Cookie]) -> bool {
        self.session_cookie(cookies).is_some()
    }

    fn active_cookie<'a>(&self, cookies: &'a [Cookie], name: &str) -> Option<&'a Cookie> {
        cookies.iter().find(|cookie| {
            cookie.name == name
                && self.cookie_matches_domain(cookie)
                && cookie.expires.is_none_or(|expires| expires > Utc::now())
        })
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
    fn from_str_round_trips_for_all_variants() {
        for p in Platform::ALL {
            assert_eq!(Platform::from_str(p.as_str()), Some(*p));
        }
    }

    #[test]
    fn from_str_is_case_insensitive() {
        assert_eq!(Platform::from_str("Instagram"), Some(Platform::Instagram));
        assert_eq!(Platform::from_str("THREADS"), Some(Platform::Threads));
        assert_eq!(Platform::from_str("NAVER"), Some(Platform::Naver));
    }

    #[test]
    fn from_str_returns_none_for_unknown() {
        assert_eq!(Platform::from_str("twitter"), None);
        assert_eq!(Platform::from_str(""), None);
    }

    #[test]
    fn naver_has_correct_cookie_domains() {
        let domains = Platform::Naver.cookie_domains();
        assert!(domains.contains(&".naver.com"));
        assert!(domains.contains(&".nid.naver.com"));
    }

    #[test]
    fn naver_session_cookie_name_is_nid_aut() {
        assert_eq!(Platform::Naver.session_cookie_name(), "NID_AUT");
    }

    #[test]
    fn naver_login_url() {
        assert_eq!(
            Platform::Naver.login_url(),
            "https://nid.naver.com/nidlogin.login"
        );
    }

    #[test]
    fn naver_filter_cookies_keeps_matching_domains() {
        let cookies = vec![
            cookie("NID_AUT", ".naver.com"),
            cookie("foo", "example.com"),
            cookie("NID_SES", ".nid.naver.com"),
        ];
        let kept = Platform::Naver.filter_cookies(cookies);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|c| c.name == "NID_AUT"));
        assert!(kept.iter().any(|c| c.name == "NID_SES"));
    }

    #[test]
    fn naver_has_session_cookie_true_when_present() {
        let cookies = vec![cookie("NID_AUT", ".naver.com")];
        assert!(Platform::Naver.has_session_cookie(&cookies));
    }

    #[test]
    fn naver_has_session_cookie_false_for_wrong_domain() {
        let cookies = vec![cookie("NID_AUT", ".example.com")];
        assert!(!Platform::Naver.has_session_cookie(&cookies));
    }

    #[test]
    fn filter_cookies_keeps_only_matching_domains() {
        let cookies = vec![
            cookie("sessionid", ".instagram.com"),
            cookie("foo", "example.com"),
            cookie("bar", ".threads.net"),
        ];
        let kept = Platform::Instagram.filter_cookies(cookies);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|c| c.domain == ".instagram.com"));
        assert!(kept.iter().any(|c| c.domain == ".threads.net"));
    }

    #[test]
    fn filter_cookies_strips_leading_dot_when_matching() {
        let cookies = vec![cookie("sessionid", "instagram.com")];
        let kept = Platform::Instagram.filter_cookies(cookies);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn filter_cookies_keeps_matching_subdomains() {
        let cookies = vec![
            cookie("sessionid", "www.instagram.com"),
            cookie("bar", "m.threads.net"),
        ];
        let kept = Platform::Threads.filter_cookies(cookies);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|c| c.domain == "www.instagram.com"));
        assert!(kept.iter().any(|c| c.domain == "m.threads.net"));
    }

    #[test]
    fn has_session_cookie_accepts_matching_subdomain() {
        let cookies = vec![cookie("sessionid", "www.instagram.com")];
        assert!(Platform::Threads.has_session_cookie(&cookies));
    }

    #[test]
    fn has_session_cookie_returns_true_when_sessionid_present() {
        let cookies = vec![cookie("sessionid", ".instagram.com")];
        assert!(Platform::Instagram.has_session_cookie(&cookies));
    }

    #[test]
    fn has_session_cookie_ignores_wrong_domain() {
        let cookies = vec![cookie("sessionid", ".example.com")];
        assert!(!Platform::Instagram.has_session_cookie(&cookies));
    }

    #[test]
    fn has_session_cookie_false_for_other_cookie_name() {
        let cookies = vec![cookie("csrftoken", ".instagram.com")];
        assert!(!Platform::Instagram.has_session_cookie(&cookies));
    }

    #[test]
    fn has_session_cookie_false_when_expired() {
        let mut session_cookie = cookie("sessionid", ".instagram.com");
        session_cookie.expires = Some(Utc::now() - Duration::seconds(1));

        assert!(!Platform::Instagram.has_session_cookie(&[session_cookie]));
    }

    #[test]
    fn has_session_cookie_accepts_session_cookie_without_expiry() {
        let mut session_cookie = cookie("sessionid", ".instagram.com");
        session_cookie.expires = None;

        assert!(Platform::Instagram.has_session_cookie(&[session_cookie]));
    }

    #[test]
    fn novelpia_login_url_and_cookie_domain() {
        assert_eq!(
            Platform::Novelpia.login_url(),
            "https://novelpia.com/login/"
        );
        assert_eq!(Platform::Novelpia.cookie_domains(), &[".novelpia.com"]);
    }

    #[test]
    fn novelpia_session_cookie_name_is_autologin() {
        assert_eq!(Platform::Novelpia.session_cookie_name(), "AUTOLOGIN");
    }

    #[test]
    fn novelpia_rejects_anonymous_bootstrap_cookies() {
        let cookies = vec![
            cookie("LOGINKEY", ".novelpia.com"),
            cookie("USERKEY", ".novelpia.com"),
        ];
        assert!(!Platform::Novelpia.has_session_cookie(&cookies));
    }

    #[test]
    fn novelpia_has_session_cookie_when_autologin_session_is_present() {
        let cookies = vec![
            cookie("LOGINKEY", ".novelpia.com"),
            cookie("USERKEY", ".novelpia.com"),
            cookie("AUTOLOGIN", ".novelpia.com"),
        ];
        assert!(Platform::Novelpia.has_session_cookie(&cookies));
    }

    #[test]
    fn novelpia_has_session_cookie_when_current_login_marker_is_present() {
        let cookies = vec![
            cookie("LOGINKEY", ".novelpia.com"),
            cookie("USERKEY", ".novelpia.com"),
            Cookie {
                value: "1".into(),
                ..cookie("ISLOGIN", ".novelpia.com")
            },
        ];
        assert!(Platform::Novelpia.has_session_cookie(&cookies));
    }

    #[test]
    fn novelpia_rejects_invalid_current_login_marker() {
        let cookies = vec![
            cookie("LOGINKEY", ".novelpia.com"),
            cookie("USERKEY", ".novelpia.com"),
            Cookie {
                value: "0".into(),
                ..cookie("ISLOGIN", ".novelpia.com")
            },
        ];
        assert!(!Platform::Novelpia.has_session_cookie(&cookies));
    }

    #[test]
    fn novelpia_rejects_autologin_without_identity_cookies() {
        let cookies = vec![cookie("AUTOLOGIN", ".novelpia.com")];
        assert!(!Platform::Novelpia.has_session_cookie(&cookies));
    }

    #[test]
    fn novelpia_filter_cookies_keeps_only_novelpia_domains() {
        let cookies = vec![
            cookie("LOGINKEY", ".novelpia.com"),
            cookie("USERKEY", "novelpia.com"),
            cookie("foo", "example.com"),
        ];
        let kept = Platform::Novelpia.filter_cookies(cookies);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|c| c.name != "foo"));
    }

    #[test]
    fn munpia_login_url_and_cookie_domain() {
        assert_eq!(
            Platform::Munpia.login_url(),
            "https://nssl.munpia.com/login"
        );
        assert_eq!(Platform::Munpia.cookie_domains(), &[".munpia.com"]);
    }

    #[test]
    fn munpia_session_cookie_name_is_token() {
        assert_eq!(Platform::Munpia.session_cookie_name(), "TOKEN");
    }

    #[test]
    fn munpia_has_session_cookie_accepts_login_subdomain() {
        let cookies = vec![cookie("TOKEN", ".nssl.munpia.com")];
        assert!(Platform::Munpia.has_session_cookie(&cookies));
    }
}
