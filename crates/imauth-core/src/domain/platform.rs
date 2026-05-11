use crate::domain::session::Cookie;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Instagram,
    Threads,
}

impl Platform {
    pub const ALL: &'static [Platform] = &[Platform::Instagram, Platform::Threads];

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "instagram" => Some(Platform::Instagram),
            "threads" => Some(Platform::Threads),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Instagram => "instagram",
            Platform::Threads => "threads",
        }
    }

    pub fn cookie_domains(&self) -> &'static [&'static str] {
        match self {
            Platform::Instagram | Platform::Threads => {
                &[".instagram.com", ".threads.net", ".threads.com"]
            }
        }
    }

    pub fn session_cookie_name(&self) -> &'static str {
        match self {
            Platform::Instagram | Platform::Threads => "sessionid",
        }
    }

    fn cookie_matches_domain(&self, cookie: &Cookie) -> bool {
        let cookie_domain = cookie.domain.trim_start_matches('.');
        self.cookie_domains()
            .iter()
            .any(|allow| cookie_domain.eq_ignore_ascii_case(allow.trim_start_matches('.')))
    }

    pub fn filter_cookies(&self, cookies: Vec<Cookie>) -> Vec<Cookie> {
        cookies
            .into_iter()
            .filter(|c| self.cookie_matches_domain(c))
            .collect()
    }

    pub fn has_session_cookie(&self, cookies: &[Cookie]) -> bool {
        let name = self.session_cookie_name();
        cookies
            .iter()
            .any(|c| c.name == name && self.cookie_matches_domain(c))
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
    }

    #[test]
    fn from_str_returns_none_for_unknown() {
        assert_eq!(Platform::from_str("twitter"), None);
        assert_eq!(Platform::from_str(""), None);
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
}
