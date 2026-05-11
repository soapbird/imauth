pub mod instagram;
pub mod selectors;

use crate::session::state::Cookie;

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
