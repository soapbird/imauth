use crate::adapters::chromiumoxide::page_driver::ChromiumOxidePageDriver;
use crate::ports::browser::{BrowserSession, BrowserSessionFactory, PageDriver};
use crate::ImauthError;
use crate::Result;
use async_trait::async_trait;
use metrics::histogram;
use chromiumoxide::Browser;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// A single Chrome instance with its own CDP connection, semaphore, and noVNC URL.
pub struct ChromeSlot {
    cdp_url: String,
    semaphore: Arc<Semaphore>,
    browsers: Arc<Mutex<Vec<Browser>>>,
    viewer_url: String,
}

impl ChromeSlot {
    pub fn new(cdp_url: String, viewer_url: String) -> Self {
        Self {
            cdp_url,
            semaphore: Arc::new(Semaphore::new(1)), // 1 concurrent session per slot
            browsers: Arc::new(Mutex::new(Vec::new())),
            viewer_url,
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

/// Pool of Chrome slots. Each slot is an independent Chrome instance with its
/// own noVNC URL. Login acquires a free slot, login completion releases it.
pub struct PooledBrowserFactory {
    slots: Vec<Arc<ChromeSlot>>,
}

impl PooledBrowserFactory {
    pub fn new(cdp_urls: Vec<String>, novnc_base_url: &str, novnc_ports: &[u16]) -> Self {
        let slots = cdp_urls
            .iter()
            .enumerate()
            .map(|(i, cdp_url)| {
                let viewer_url = if i < novnc_ports.len() {
                    format!("{}:{}/vnc.html?autoconnect=true&clipboard_seamless=1&clipboard_up=1&clipboard_down=1", novnc_base_url, novnc_ports[i])
                } else if novnc_ports.is_empty() {
                    novnc_base_url.to_string()
                } else {
                    format!(
                        "{}:{}/vnc.html?autoconnect=true&clipboard_seamless=1&clipboard_up=1&clipboard_down=1",
                        novnc_base_url,
                        novnc_ports[novnc_ports.len() - 1]
                    )
                };
                Arc::new(ChromeSlot::new(cdp_url.clone(), viewer_url))
            })
            .collect();

        Self { slots }
    }
}

#[async_trait]
impl BrowserSessionFactory for PooledBrowserFactory {
    async fn acquire(&self) -> Result<Box<dyn BrowserSession>> {
        let start = std::time::Instant::now();
        // Try each slot in order; first available wins.
        for slot in &self.slots {
            if let Ok(permit) = slot.semaphore.clone().try_acquire_owned() {
                let mut browsers = slot.browsers.lock().await;
                let browser = match browsers.pop() {
                    Some(b) => b,
                    None => {
                        drop(browsers);
                        match ChromeSlot::connect(&slot.cdp_url).await {
                            Ok(b) => b,
                            Err(_e) => continue, // try next slot
                        }
                    }
                };

                return Ok(Box::new(ChromiumOxideBrowserSession {
                    browser: Some(browser),
                    pool: slot.browsers.clone(),
                    max_size: 1,
                    viewer_url: slot.viewer_url.clone(),
                    _permit: permit,
                }));
            }
        }

        // All slots busy — block on the first one.
        let slot = &self.slots[0];
        let permit = slot
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| ImauthError::Browser(format!("All browser slots busy: {e}")))?;

        let mut browsers = slot.browsers.lock().await;
        let browser = match browsers.pop() {
            Some(b) => b,
            None => {
                drop(browsers);
                ChromeSlot::connect(&slot.cdp_url).await?
            }
        };

        histogram!("browser_pool_wait_seconds").record(start.elapsed().as_secs_f64());
        Ok(Box::new(ChromiumOxideBrowserSession {
            browser: Some(browser),
            pool: slot.browsers.clone(),
            max_size: 1,
            viewer_url: slot.viewer_url.clone(),
            _permit: permit,
        }))
    }

    fn viewer_url(&self) -> Option<String> {
        // Return the first slot's URL as default; actual per-slot URL
        // is returned via the session's viewer_url field.
        self.slots.first().map(|s| s.viewer_url.clone())
    }
}

/// A held browser that returns itself to the pool on drop (RAII).
pub struct ChromiumOxideBrowserSession {
    browser: Option<Browser>,
    pool: Arc<Mutex<Vec<Browser>>>,
    max_size: usize,
    viewer_url: String,
    _permit: OwnedSemaphorePermit,
}

impl ChromiumOxideBrowserSession {
    fn inner(&self) -> Result<&Browser> {
        self.browser
            .as_ref()
            .ok_or_else(|| ImauthError::Browser("browser session already released".into()))
    }

    pub fn viewer_url(&self) -> &str {
        &self.viewer_url
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

    fn viewer_url(&self) -> String {
        self.viewer_url.clone()
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
