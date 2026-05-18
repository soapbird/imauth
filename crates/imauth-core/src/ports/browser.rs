use crate::domain::session::Cookie;
use crate::Result;
use async_trait::async_trait;

/// Acquires an isolated browser session (e.g. a CDP target) from a pool.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait BrowserSessionFactory: Send + Sync {
    async fn acquire(&self) -> Result<Box<dyn BrowserSession>>;
    fn viewer_url(&self) -> Option<String>;
}

/// A live browser instance the use case can drive.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait BrowserSession: Send + Sync {
    async fn new_page(&self) -> Result<Box<dyn PageDriver>>;
    async fn existing_pages(&self) -> Result<Vec<Box<dyn PageDriver>>>;
    fn viewer_url(&self) -> String;
}

/// Page-level interactions needed for user-driven login.
/// Reduced from the full automation set: we only navigate and read cookies.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PageDriver: Send + Sync {
    async fn navigate(&self, url: &str, timeout_secs: u64) -> Result<()>;
    async fn get_cookies(&self) -> Result<Vec<Cookie>>;
    async fn screenshot(&self) -> Result<Vec<u8>>;
    async fn content_html(&self) -> Result<String>;
    async fn close(&self) -> Result<()>;
    async fn set_mobile_viewport(&self) -> Result<()>;
}
