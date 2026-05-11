pub mod instagram;
pub mod selectors;
pub mod threads;

use crate::session::state::Session;
use chromiumoxide::page::Page;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Instagram,
    Threads,
}

impl Platform {
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

    pub fn cookie_domains(&self) -> Vec<&'static str> {
        match self {
            Platform::Instagram => {
                vec![
                    ".instagram.com",
                    ".threads.net",
                    ".threads.com",
                ]
            }
            Platform::Threads => {
                vec![
                    ".instagram.com",
                    ".threads.net",
                    ".threads.com",
                ]
            }
        }
    }

    pub fn session_cookie_name(&self) -> &'static str {
        match self {
            Platform::Instagram => "sessionid",
            Platform::Threads => "sessionid",
        }
    }
}

pub enum PlatformAuth {
    Instagram,
    Threads,
}

impl PlatformAuth {
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::Instagram => PlatformAuth::Instagram,
            Platform::Threads => PlatformAuth::Threads,
        }
    }

    pub async fn login(
        &self,
        page: &Page,
        username: &str,
        password: &str,
        session: &mut Session,
    ) -> crate::Result<()> {
        match self {
            PlatformAuth::Instagram => instagram::login(page, username, password, session).await,
            PlatformAuth::Threads => threads::login(page, username, password, session).await,
        }
    }

    pub async fn submit_2fa(
        &self,
        page: &Page,
        code: &str,
        session: &mut Session,
    ) -> crate::Result<()> {
        match self {
            PlatformAuth::Instagram => instagram::submit_2fa(page, code, session).await,
            PlatformAuth::Threads => threads::submit_2fa(page, code, session).await,
        }
    }
}

pub fn get_platform_auth(platform: Platform) -> PlatformAuth {
    PlatformAuth::for_platform(platform)
}
