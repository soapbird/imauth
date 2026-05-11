use crate::browser::cdp::CdpBrowser;
use crate::ImauthError;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

pub struct BrowserPool {
    cdp_url: String,
    max_size: usize,
    semaphore: Arc<Semaphore>,
    browsers: Arc<Mutex<Vec<CdpBrowser>>>,
}

impl BrowserPool {
    pub fn new(cdp_url: String, max_size: usize) -> Self {
        Self {
            cdp_url,
            max_size,
            semaphore: Arc::new(Semaphore::new(max_size)),
            browsers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn acquire(&self,
    ) -> crate::Result<BrowserHandle> {
        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| ImauthError::Browser(format!("Semaphore error: {e}")))?;

        // Try to reuse an existing browser
        let mut browsers = self.browsers.lock().await;
        if let Some(browser) = browsers.pop() {
            return Ok(BrowserHandle {
                browser: Some(browser),
                pool: self.browsers.clone(),
                max_size: self.max_size,
                _permit,
            });
        }
        drop(browsers);

        let browser = CdpBrowser::connect(&self.cdp_url).await?;
        Ok(BrowserHandle {
            browser: Some(browser),
            pool: self.browsers.clone(),
            max_size: self.max_size,
            _permit,
        })
    }
}

pub struct BrowserHandle {
    browser: Option<CdpBrowser>,
    pool: Arc<Mutex<Vec<CdpBrowser>>>,
    max_size: usize,
    #[allow(dead_code)]
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl BrowserHandle {
    pub fn browser(&self) -> &CdpBrowser {
        self.browser.as_ref().unwrap()
    }

    pub async fn pages(&self) -> crate::Result<Vec<chromiumoxide::Page>> {
        self.browser().pages().await
    }
}

impl Drop for BrowserHandle {
    fn drop(&mut self) {
        if let Some(browser) = self.browser.take() {
            let pool = self.pool.clone();
            let max_size = self.max_size;
            tokio::spawn(async move {
                let mut browsers = pool.lock().await;
                if browsers.len() < max_size {
                    browsers.push(browser);
                }
                // If pool is at capacity, browser is dropped/closed
            });
        }
    }
}
