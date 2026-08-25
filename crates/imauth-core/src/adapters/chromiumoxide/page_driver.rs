use crate::domain::session::Cookie;
use crate::ports::browser::PageDriver;
use crate::ImauthError;
use crate::Result;
use async_trait::async_trait;
use chromiumoxide::page::Page;
use serde::de::DeserializeOwned;
use std::time::Duration;

const COOKIE_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ChromiumOxidePageDriver {
    page: tokio::sync::Mutex<Option<Page>>,
}

impl ChromiumOxidePageDriver {
    pub fn new(page: Page) -> Self {
        Self {
            page: tokio::sync::Mutex::new(Some(page)),
        }
    }

    async fn eval_into<T: DeserializeOwned>(&self, js: impl Into<String>, ctx: &str) -> Result<T> {
        let guard = self.page.lock().await;
        let page = guard
            .as_ref()
            .ok_or_else(|| ImauthError::Browser(format!("{ctx}: page already closed")))?;
        page.evaluate(js.into())
            .await
            .map_err(|e| ImauthError::Browser(format!("{ctx} eval failed: {e}")))?
            .into_value()
            .map_err(|e| ImauthError::Browser(format!("{ctx} result parse failed: {e}")))
    }
}

#[async_trait]
impl PageDriver for ChromiumOxidePageDriver {
    async fn navigate(&self, url: &str, timeout_secs: u64) -> Result<()> {
        let guard = self.page.lock().await;
        let page = guard
            .as_ref()
            .ok_or_else(|| ImauthError::Browser("navigate: page already closed".into()))?;

        let nav = async {
            page.goto(url)
                .await
                .map_err(|e| ImauthError::Browser(format!("Navigation failed: {e}")))?;
            page.wait_for_navigation()
                .await
                .map_err(|e| ImauthError::Browser(format!("Wait for navigation failed: {e}")))?;
            Ok::<(), ImauthError>(())
        };
        tokio::time::timeout(Duration::from_secs(timeout_secs), nav)
            .await
            .map_err(|_| {
                ImauthError::Browser(format!(
                    "Navigation to {url} timed out after {timeout_secs}s"
                ))
            })??;
        Ok(())
    }

    async fn get_cookies(&self) -> Result<Vec<Cookie>> {
        let cookies = tokio::time::timeout(COOKIE_READ_TIMEOUT, async {
            let guard = self.page.lock().await;
            let page = guard
                .as_ref()
                .ok_or_else(|| ImauthError::Browser("get_cookies: page already closed".into()))?;
            page.get_cookies()
                .await
                .map_err(|e| ImauthError::Browser(format!("Failed to get cookies: {e}")))
        })
        .await
        .map_err(|_| {
            ImauthError::Browser(format!(
                "Cookie read timed out after {}s",
                COOKIE_READ_TIMEOUT.as_secs()
            ))
        })??;

        Ok(cookies
            .into_iter()
            .map(|c| Cookie {
                name: c.name,
                value: c.value,
                domain: c.domain,
                path: c.path,
                expires: if c.expires > 0.0 {
                    chrono::DateTime::from_timestamp(c.expires as i64, 0)
                } else {
                    None
                },
                http_only: c.http_only,
                secure: c.secure,
            })
            .collect())
    }

    async fn screenshot(&self) -> Result<Vec<u8>> {
        let guard = self.page.lock().await;
        let page = guard
            .as_ref()
            .ok_or_else(|| ImauthError::Browser("screenshot: page already closed".into()))?;
        let params = chromiumoxide::page::ScreenshotParams::builder().build();
        page.screenshot(params)
            .await
            .map_err(|e| ImauthError::Browser(format!("Screenshot failed: {e}")))
    }

    async fn content_html(&self) -> Result<String> {
        self.eval_into("() => document.documentElement.outerHTML", "content_html")
            .await
    }

    async fn close(&self) -> Result<()> {
        let mut guard = self.page.lock().await;
        if let Some(page) = guard.take() {
            page.close()
                .await
                .map_err(|e| ImauthError::Browser(format!("Failed to close page: {e}")))?;
        }
        Ok(())
    }
}
