use crate::Result;
use crate::domain::session::{Cookie, Session};
use crate::domain::Platform;
use crate::ports::snapshot::SnapshotSink;
use async_trait::async_trait;

/// Acquires an isolated browser session (e.g. a CDP target) from a pool.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait BrowserSessionFactory: Send + Sync {
    async fn acquire(&self) -> Result<Box<dyn BrowserSession>>;
}

/// A live browser instance the use case can drive.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait BrowserSession: Send + Sync {
    async fn new_page(&self) -> Result<Box<dyn PageDriver>>;
    async fn existing_pages(&self) -> Result<Vec<Box<dyn PageDriver>>>;
}

/// Page-level interactions a platform driver needs. Hides chromiumoxide entirely
/// so the orchestration logic can be unit-tested with a mock.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PageDriver: Send + Sync {
    async fn navigate(&self, url: &str, timeout_secs: u64) -> Result<()>;
    async fn find_element(&self, selector: &str) -> Result<bool>;
    async fn fill_input(&self, selector: &str, value: &str) -> Result<()>;
    async fn click_element(&self, selector: &str) -> Result<()>;
    async fn click_element_text(&self, selector: &str, texts: &[String]) -> Result<bool>;
    async fn press_enter(&self, selector: &str) -> Result<()>;
    async fn get_page_text(&self) -> Result<String>;
    async fn get_cookies(&self) -> Result<Vec<Cookie>>;
    async fn screenshot(&self) -> Result<Vec<u8>>;
    async fn content_html(&self) -> Result<String>;
}

/// Platform-specific login orchestration (Instagram, Threads, ...).
/// Consumes a generic `PageDriver` so the state machine is testable without a browser.
// Note: `PlatformDriver` is intentionally NOT `automock`'d — mockall + async_trait
// can't generate matching lifetime parameters for multi-reference async methods.
// Tests use hand-written stub implementations (see `application::login::tests`).
#[async_trait]
pub trait PlatformDriver: Send + Sync {
    async fn login<'a>(
        &'a self,
        page: &'a dyn PageDriver,
        platform: Platform,
        username: &'a str,
        password: &'a str,
        session: &'a mut Session,
        snapshot: &'a dyn SnapshotSink,
    ) -> Result<Vec<Cookie>>;

    async fn submit_2fa<'a>(
        &'a self,
        page: &'a dyn PageDriver,
        platform: Platform,
        code: &'a str,
        session: &'a mut Session,
        snapshot: &'a dyn SnapshotSink,
    ) -> Result<Vec<Cookie>>;
}
