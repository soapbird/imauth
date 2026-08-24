use crate::domain::session::Cookie;
use crate::domain::Platform;
use crate::ports::repository::CookieRepository;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ValidationOutcome {
    pub valid: bool,
    pub expires_at: i64,
}

pub struct GetCookiesUseCase {
    cookies: Arc<dyn CookieRepository>,
}

impl GetCookiesUseCase {
    pub fn new(cookies: Arc<dyn CookieRepository>) -> Self {
        Self { cookies }
    }

    pub async fn execute(
        &self,
        platform: Platform,
        domains: Option<Vec<String>>,
    ) -> Result<Vec<Cookie>> {
        self.cookies
            .get(platform.as_str(), domains.as_deref())
            .await
    }
}

pub struct UpdateCookiesUseCase {
    cookies: Arc<dyn CookieRepository>,
}

impl UpdateCookiesUseCase {
    pub fn new(cookies: Arc<dyn CookieRepository>) -> Self {
        Self { cookies }
    }

    pub async fn execute(&self, platform: Platform, cookies: Vec<Cookie>) -> Result<()> {
        self.cookies.save(platform.as_str(), &cookies).await
    }
}

pub struct ExportNetscapeUseCase {
    cookies: Arc<dyn CookieRepository>,
}

impl ExportNetscapeUseCase {
    pub fn new(cookies: Arc<dyn CookieRepository>) -> Self {
        Self { cookies }
    }

    pub async fn execute(&self, platform: Platform) -> Result<String> {
        self.cookies.export_netscape(platform.as_str()).await
    }
}

pub struct ValidateSessionUseCase {
    cookies: Arc<dyn CookieRepository>,
}

impl ValidateSessionUseCase {
    pub fn new(cookies: Arc<dyn CookieRepository>) -> Self {
        Self { cookies }
    }

    pub async fn execute(&self, platform: Platform) -> Result<ValidationOutcome> {
        let cookies = self.cookies.get(platform.as_str(), None).await?;
        let session_cookie = platform.session_cookie(&cookies);
        Ok(ValidationOutcome {
            valid: session_cookie.is_some(),
            expires_at: session_cookie
                .and_then(|c| c.expires)
                .map(|dt| dt.timestamp())
                .unwrap_or(0),
        })
    }
}

pub struct GetConnectionStatusUseCase {
    cookies: Arc<dyn CookieRepository>,
}

impl GetConnectionStatusUseCase {
    pub fn new(cookies: Arc<dyn CookieRepository>) -> Self {
        Self { cookies }
    }

    pub async fn execute(&self) -> Result<HashMap<String, bool>> {
        let jar = &self.cookies;
        let fetches = Platform::ALL.iter().map(|p| async move {
            let c = jar.get(p.as_str(), None).await.unwrap_or_default();
            (*p, c)
        });
        let results = futures::future::join_all(fetches).await;
        Ok(results
            .into_iter()
            .map(|(p, c)| (p.as_str().to_string(), p.has_session_cookie(&c)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::repository::MockCookieRepository;
    use chrono::{Duration, TimeZone, Utc};

    fn cookie(name: &str, domain: &str, expires_ts: Option<i64>) -> Cookie {
        Cookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            expires: expires_ts.and_then(|ts| chrono::Utc.timestamp_opt(ts, 0).single()),
            http_only: true,
            secure: true,
        }
    }

    #[tokio::test]
    async fn get_cookies_forwards_platform_string_and_domains() {
        let mut repo = MockCookieRepository::new();
        repo.expect_get()
            .withf(|p, d| {
                p == "instagram" && d.map(|d| d.to_vec()) == Some(vec!["a.com".to_string()])
            })
            .return_once(|_, _| Ok(vec![]));
        let uc = GetCookiesUseCase::new(Arc::new(repo));
        let out = uc
            .execute(Platform::Instagram, Some(vec!["a.com".into()]))
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn validate_returns_true_with_expires_when_sessionid_present() {
        let expires_at = (Utc::now() + Duration::hours(1)).timestamp();
        let mut repo = MockCookieRepository::new();
        repo.expect_get().return_once(move |_, _| {
            Ok(vec![cookie(
                "sessionid",
                ".instagram.com",
                Some(expires_at),
            )])
        });
        let uc = ValidateSessionUseCase::new(Arc::new(repo));
        let outcome = uc.execute(Platform::Instagram).await.unwrap();
        assert!(outcome.valid);
        assert_eq!(outcome.expires_at, expires_at);
    }

    #[tokio::test]
    async fn validate_returns_false_when_session_cookie_missing() {
        let mut repo = MockCookieRepository::new();
        repo.expect_get()
            .return_once(|_, _| Ok(vec![cookie("csrftoken", ".instagram.com", None)]));
        let uc = ValidateSessionUseCase::new(Arc::new(repo));
        let outcome = uc.execute(Platform::Instagram).await.unwrap();
        assert!(!outcome.valid);
        assert_eq!(outcome.expires_at, 0);
    }

    #[tokio::test]
    async fn validate_returns_false_for_expired_session_cookie() {
        let expires_at = (Utc::now() - Duration::hours(1)).timestamp();
        let mut repo = MockCookieRepository::new();
        repo.expect_get().return_once(move |_, _| {
            Ok(vec![cookie(
                "sessionid",
                ".instagram.com",
                Some(expires_at),
            )])
        });
        let uc = ValidateSessionUseCase::new(Arc::new(repo));

        let outcome = uc.execute(Platform::Instagram).await.unwrap();

        assert!(!outcome.valid);
        assert_eq!(outcome.expires_at, 0);
    }

    #[tokio::test]
    async fn validate_returns_false_for_session_cookie_on_wrong_domain() {
        let mut repo = MockCookieRepository::new();
        repo.expect_get()
            .return_once(|_, _| Ok(vec![cookie("sessionid", ".example.com", None)]));
        let uc = ValidateSessionUseCase::new(Arc::new(repo));

        let outcome = uc.execute(Platform::Instagram).await.unwrap();

        assert!(!outcome.valid);
        assert_eq!(outcome.expires_at, 0);
    }

    #[tokio::test]
    async fn connection_status_reports_both_platforms() {
        let mut repo = MockCookieRepository::new();
        repo.expect_get().returning(|platform, _| match platform {
            "instagram" => Ok(vec![cookie("sessionid", ".instagram.com", None)]),
            _ => Ok(vec![]),
        });
        let uc = GetConnectionStatusUseCase::new(Arc::new(repo));
        let map = uc.execute().await.unwrap();
        assert_eq!(map.get("instagram"), Some(&true));
        assert_eq!(map.get("threads"), Some(&false));
    }
}
