use crate::domain::session::{Cookie, Session};
use crate::domain::Credential;
use crate::Result;
use async_trait::async_trait;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(&self, session: Session) -> Result<Session>;
    async fn get(&self, id: &str) -> Result<Option<Session>>;
    async fn update(&self, session: &Session) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait CookieRepository: Send + Sync {
    async fn save(&self, platform: &str, cookies: &[Cookie]) -> Result<()>;
    async fn get<'a>(
        &'a self,
        platform: &'a str,
        domains: Option<&'a [String]>,
    ) -> Result<Vec<Cookie>>;
    async fn export_netscape(&self, platform: &str) -> Result<String>;
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait CredentialRepository: Send + Sync {
    async fn save<'a>(
        &'a self,
        platform: &'a str,
        username: &'a str,
        password: &'a str,
        twofa_method: Option<&'a str>,
    ) -> Result<()>;
    async fn get(&self, platform: &str) -> Result<Option<Credential>>;
    async fn delete(&self, platform: &str) -> Result<()>;
}
