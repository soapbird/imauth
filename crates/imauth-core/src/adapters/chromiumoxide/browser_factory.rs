use crate::adapters::chromiumoxide::page_driver::ChromiumOxidePageDriver;
use crate::ports::browser::{BrowserSession, BrowserSessionFactory, PageDriver};
use crate::ImauthError;
use crate::Result;
use async_trait::async_trait;
use chromiumoxide::Browser;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// A single Chrome instance with its own CDP connection, semaphore, and viewer URL.
pub struct ChromeSlot {
    cdp_url: String,
    semaphore: Arc<Semaphore>,
    viewer_url: String,
}

impl ChromeSlot {
    pub fn new(cdp_url: String, viewer_url: String) -> Self {
        Self {
            cdp_url,
            semaphore: Arc::new(Semaphore::new(1)), // 1 concurrent session per slot
            viewer_url,
        }
    }

    /// Chromium 131+ rejects CDP HTTP requests when the Host header contains
    /// a non-IP hostname (e.g. `chrome-0`). Resolve the hostname to an IP so
    /// the initial `json/version` request succeeds inside Docker networks.
    async fn resolve_cdp_url(cdp_url: &str) -> Result<String> {
        let url = url::Url::parse(cdp_url)
            .map_err(|e| ImauthError::Browser(format!("Invalid CDP URL: {e}")))?;
        let host = url
            .host_str()
            .ok_or_else(|| ImauthError::Browser("CDP URL has no host".into()))?;

        // If already an IP, nothing to do.
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(cdp_url.to_string());
        }

        let addrs: Vec<_> =
            tokio::net::lookup_host(format!("{}:{}", host, url.port().unwrap_or(9222)))
                .await
                .map_err(|e| ImauthError::Browser(format!("Failed to resolve {host}: {e}")))?
                .collect();

        let addr = addrs
            .first()
            .ok_or_else(|| ImauthError::Browser(format!("No addresses for {host}")))?;

        let mut resolved = url.clone();
        resolved
            .set_host(Some(&addr.ip().to_string()))
            .map_err(|e| ImauthError::Browser(format!("Failed to set host: {e}")))?;
        Ok(resolved.to_string())
    }

    async fn connect(cdp_url: &str) -> Result<Browser> {
        let resolved = Self::resolve_cdp_url(cdp_url).await?;
        tracing::debug!("Connecting to CDP at {resolved} (original: {cdp_url})");
        let (browser, mut handler) = Browser::connect(&resolved)
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
/// own viewer URL. Login acquires a free slot, login completion releases it.
pub struct PooledBrowserFactory {
    slots: Vec<Arc<ChromeSlot>>,
    acquire_timeout: Duration,
}

impl PooledBrowserFactory {
    pub fn new(cdp_urls: Vec<String>, viewer_urls: &[String], acquire_timeout: Duration) -> Self {
        let slots = cdp_urls
            .iter()
            .enumerate()
            .map(|(i, cdp_url)| {
                let viewer_url = viewer_urls.get(i).cloned().unwrap_or_default();
                Arc::new(ChromeSlot::new(cdp_url.clone(), viewer_url))
            })
            .collect();

        Self {
            slots,
            acquire_timeout,
        }
    }

    async fn connect(&self, slot: &ChromeSlot) -> Result<Browser> {
        tokio::time::timeout(self.acquire_timeout, ChromeSlot::connect(&slot.cdp_url))
            .await
            .map_err(|_| {
                ImauthError::Browser(format!(
                    "CDP connection timed out after {}s",
                    self.acquire_timeout.as_secs()
                ))
            })?
    }
}

#[async_trait]
impl BrowserSessionFactory for PooledBrowserFactory {
    async fn acquire(&self) -> Result<Box<dyn BrowserSession>> {
        // Try each slot in order; first available wins.
        for slot in &self.slots {
            if let Ok(permit) = slot.semaphore.clone().try_acquire_owned() {
                let browser = match self.connect(slot).await {
                    Ok(b) => b,
                    Err(error) => {
                        tracing::warn!(cdp_url = %slot.cdp_url, %error, "failed to connect to browser slot");
                        continue;
                    }
                };

                return Ok(Box::new(ChromiumOxideBrowserSession {
                    browser: Some(browser),
                    viewer_url: slot.viewer_url.clone(),
                    _permit: permit,
                }));
            }
        }

        // All slots busy — block on the first one.
        let slot = self
            .slots
            .first()
            .ok_or_else(|| ImauthError::Browser("No browser slots configured".into()))?;
        let permit =
            tokio::time::timeout(self.acquire_timeout, slot.semaphore.clone().acquire_owned())
                .await
                .map_err(|_| {
                    ImauthError::Browser(format!(
                        "Browser slot acquisition timed out after {}s",
                        self.acquire_timeout.as_secs()
                    ))
                })?
                .map_err(|e| ImauthError::Browser(format!("All browser slots busy: {e}")))?;

        let browser = self.connect(slot).await?;

        Ok(Box::new(ChromiumOxideBrowserSession {
            browser: Some(browser),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_viewer_urls_are_used_without_legacy_novnc_fallback() {
        let viewer_urls = vec![
            "http://localhost:6101/index.html".to_string(),
            "http://localhost:6102/index.html".to_string(),
            "http://localhost:6103/index.html".to_string(),
        ];
        let factory = PooledBrowserFactory::new(
            vec![
                "http://chrome-0:9223".to_string(),
                "http://chrome-1:9223".to_string(),
                "http://chrome-2:9223".to_string(),
            ],
            &viewer_urls,
            Duration::from_secs(30),
        );

        assert_eq!(
            factory.slots[0].viewer_url,
            "http://localhost:6101/index.html"
        );
        assert_eq!(
            factory.slots[1].viewer_url,
            "http://localhost:6102/index.html"
        );
        assert_eq!(
            factory.slots[2].viewer_url,
            "http://localhost:6103/index.html"
        );
    }

    #[test]
    fn empty_viewer_urls_do_not_generate_legacy_novnc_urls() {
        let factory = PooledBrowserFactory::new(
            vec!["http://chrome-0:9223".to_string()],
            &[],
            Duration::from_secs(30),
        );

        assert_eq!(factory.viewer_url().as_deref(), Some(""));
    }

    #[test]
    fn missing_viewer_urls_are_not_reused_for_later_slots() {
        let viewer_urls = vec!["http://localhost:6101/index.html".to_string()];
        let factory = PooledBrowserFactory::new(
            vec![
                "http://chrome-0:9223".to_string(),
                "http://chrome-1:9223".to_string(),
            ],
            &viewer_urls,
            Duration::from_secs(30),
        );

        assert_eq!(factory.slots[0].viewer_url, viewer_urls[0]);
        assert!(factory.slots[1].viewer_url.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_times_out_when_every_slot_is_busy() {
        let factory = PooledBrowserFactory::new(
            vec!["http://chrome-0:9223".to_string()],
            &[],
            Duration::from_secs(1),
        );
        let _permit = factory.slots[0]
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap();

        let result = factory.acquire().await;

        assert!(matches!(
            result,
            Err(ImauthError::Browser(message)) if message.contains("acquisition timed out")
        ));
    }
}

/// A held browser connection for a single login attempt (RAII).
pub struct ChromiumOxideBrowserSession {
    browser: Option<Browser>,
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
        drop(browser);
    }
}
