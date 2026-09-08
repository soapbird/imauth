use crate::domain::session::Cookie;
use crate::ports::browser::PageDriver;
use crate::ImauthError;
use crate::Result;
use async_trait::async_trait;
use chromiumoxide::page::Page;
use std::future::Future;
use std::time::Duration;
use tokio::sync::watch;

const COOKIE_READ_TIMEOUT: Duration = Duration::from_secs(10);
const PAGE_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ChromiumOxidePageDriver {
    page: tokio::sync::Mutex<Option<Page>>,
    disconnected: watch::Receiver<bool>,
}

async fn run_while_connected<T>(
    mut disconnected: watch::Receiver<bool>,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    if *disconnected.borrow() {
        return Err(ImauthError::Browser("CDP handler disconnected".into()));
    }
    tokio::select! {
        biased;
        _ = disconnected.changed() => {
            Err(ImauthError::Browser("CDP handler disconnected".into()))
        }
        result = operation => result,
    }
}

impl ChromiumOxidePageDriver {
    pub fn new(page: Page, disconnected: watch::Receiver<bool>) -> Self {
        Self {
            page: tokio::sync::Mutex::new(Some(page)),
            disconnected,
        }
    }
}

#[async_trait]
impl PageDriver for ChromiumOxidePageDriver {
    async fn navigate(&self, url: &str, timeout_secs: u64) -> Result<()> {
        tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            run_while_connected(self.disconnected.clone(), async {
                let guard = self.page.lock().await;
                let page = guard
                    .as_ref()
                    .ok_or_else(|| ImauthError::Browser("navigate: page already closed".into()))?;
                page.goto(url)
                    .await
                    .map_err(|e| ImauthError::Browser(format!("Navigation failed: {e}")))?;
                page.wait_for_navigation().await.map_err(|e| {
                    ImauthError::Browser(format!("Wait for navigation failed: {e}"))
                })?;
                Ok(())
            }),
        )
        .await
        .map_err(|_| {
            ImauthError::Browser(format!(
                "Navigation to {url} timed out after {timeout_secs}s"
            ))
        })??;
        Ok(())
    }

    async fn get_cookies(&self) -> Result<Vec<Cookie>> {
        let cookies = tokio::time::timeout(
            COOKIE_READ_TIMEOUT,
            run_while_connected(self.disconnected.clone(), async {
                let guard = self.page.lock().await;
                let page = guard.as_ref().ok_or_else(|| {
                    ImauthError::Browser("get_cookies: page already closed".into())
                })?;
                page.get_cookies()
                    .await
                    .map_err(|e| ImauthError::Browser(format!("Failed to get cookies: {e}")))
            }),
        )
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
        run_while_connected(self.disconnected.clone(), async {
            let guard = self.page.lock().await;
            let page = guard
                .as_ref()
                .ok_or_else(|| ImauthError::Browser("screenshot: page already closed".into()))?;
            let params = chromiumoxide::page::ScreenshotParams::builder().build();
            page.screenshot(params)
                .await
                .map_err(|e| ImauthError::Browser(format!("Screenshot failed: {e}")))
        })
        .await
    }

    async fn content_html(&self) -> Result<String> {
        run_while_connected(self.disconnected.clone(), async {
            let guard = self.page.lock().await;
            let page = guard
                .as_ref()
                .ok_or_else(|| ImauthError::Browser("content_html: page already closed".into()))?;
            page.evaluate("() => document.documentElement.outerHTML")
                .await
                .map_err(|e| ImauthError::Browser(format!("content_html eval failed: {e}")))?
                .into_value()
                .map_err(|e| ImauthError::Browser(format!("content_html result parse failed: {e}")))
        })
        .await
    }

    async fn close(&self) -> Result<()> {
        tokio::time::timeout(PAGE_CLOSE_TIMEOUT, async {
            let mut guard = self.page.lock().await;
            if let Some(page) = guard.take() {
                page.close()
                    .await
                    .map_err(|e| ImauthError::Browser(format!("Failed to close page: {e}")))?;
            }
            Ok(())
        })
        .await
        .map_err(|_| ImauthError::Browser("Page close timed out".into()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pending_page_operation_fails_when_handler_disconnects() {
        // Given a page operation waiting on a live handler.
        let (sender, receiver) = watch::channel(false);
        let operation = run_while_connected(receiver, std::future::pending::<Result<()>>());

        // When the handler reports disconnection.
        sender.send(true).unwrap();
        let result = operation.await;

        // Then the operation fails immediately with the connection cause.
        assert!(matches!(
            result,
            Err(ImauthError::Browser(message)) if message == "CDP handler disconnected"
        ));
    }
}
