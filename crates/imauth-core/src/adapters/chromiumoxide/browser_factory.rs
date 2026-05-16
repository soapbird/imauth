use crate::adapters::chromiumoxide::page_driver::ChromiumOxidePageDriver;
use crate::ports::browser::{BrowserSession, BrowserSessionFactory, PageDriver};
use crate::ImauthError;
use crate::Result;
use async_trait::async_trait;
use chromiumoxide::Browser;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// Pool of pre-connected CDP browsers, plus a semaphore that caps concurrent acquires.
pub struct ChromiumOxideBrowserFactory {
    cdp_url: String,
    max_size: usize,
    semaphore: Arc<Semaphore>,
    browsers: Arc<Mutex<Vec<Browser>>>,
}

impl ChromiumOxideBrowserFactory {
    pub fn new(cdp_url: String, max_size: usize) -> Self {
        Self {
            cdp_url,
            max_size,
            semaphore: Arc::new(Semaphore::new(max_size)),
            browsers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn connect(cdp_url: &str) -> Result<Browser> {
        let (browser, mut handler) = Browser::connect(cdp_url)
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to connect to CDP: {e}")))?;
        tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if let Err(e) = h {
                    tracing::warn!("CDP handler stopped: {e}");
                    break;
                }
            }
        });
        Ok(browser)
    }
}

#[async_trait]
impl BrowserSessionFactory for ChromiumOxideBrowserFactory {
    async fn acquire(&self) -> Result<Box<dyn BrowserSession>> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| ImauthError::Browser(format!("Semaphore error: {e}")))?;

        let mut browsers = self.browsers.lock().await;
        let browser = match browsers.pop() {
            Some(b) => b,
            None => {
                drop(browsers);
                Self::connect(&self.cdp_url).await?
            }
        };

        Ok(Box::new(ChromiumOxideBrowserSession {
            browser: Some(browser),
            pool: self.browsers.clone(),
            max_size: self.max_size,
            _permit: permit,
        }))
    }
}

/// A held browser that returns itself to the pool on drop (RAII).
pub struct ChromiumOxideBrowserSession {
    browser: Option<Browser>,
    pool: Arc<Mutex<Vec<Browser>>>,
    max_size: usize,
    _permit: OwnedSemaphorePermit,
}

impl ChromiumOxideBrowserSession {
    /// Returns the wrapped browser handle. Result-returning so that a future
    /// code path that takes the browser out via `take()` outside of `Drop`
    /// surfaces a typed error instead of panicking inside an async task and
    /// leaking the pool permit forever.
    fn inner(&self) -> Result<&Browser> {
        self.browser
            .as_ref()
            .ok_or_else(|| ImauthError::Browser("browser session already released".into()))
    }
}

#[async_trait]
impl BrowserSession for ChromiumOxideBrowserSession {
    async fn new_page(&self) -> Result<Box<dyn PageDriver>> {
        let page = self
            .inner()?
            .new_page("about:blank")
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to create page: {e}")))?;
        Ok(Box::new(ChromiumOxidePageDriver::new(page)))
    }

    async fn existing_pages(&self) -> Result<Vec<Box<dyn PageDriver>>> {
        let pages = self
            .inner()?
            .pages()
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to list pages: {e}")))?;
        Ok(pages
            .into_iter()
            .map(|p| Box::new(ChromiumOxidePageDriver::new(p)) as Box<dyn PageDriver>)
            .collect())
    }
}

impl Drop for ChromiumOxideBrowserSession {
    fn drop(&mut self) {
        let Some(browser) = self.browser.take() else {
            return;
        };
        if tokio::runtime::Handle::try_current().is_err() {
            tracing::warn!("Tokio runtime gone during browser drop; browser will close");
            return;
        }
        let pool = self.pool.clone();
        let max_size = self.max_size;
        tokio::spawn(async move {
            let mut browsers = pool.lock().await;
            if browsers.len() < max_size {
                browsers.push(browser);
            }
        });
    }
}
