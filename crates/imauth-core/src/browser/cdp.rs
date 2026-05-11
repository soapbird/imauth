use crate::ImauthError;
use chromiumoxide::page::Page;
use futures::StreamExt;
use std::time::Duration;

pub struct CdpBrowser {
    browser: chromiumoxide::Browser,
}

impl CdpBrowser {
    pub async fn connect(cdp_url: &str) -> crate::Result<Self> {
        let (browser, mut handler) = chromiumoxide::Browser::connect(cdp_url)
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to connect to CDP: {e}")))?;

        // Spawn handler task
        tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        Ok(Self { browser })
    }

    pub async fn new_page(&self,
    ) -> crate::Result<chromiumoxide::Page> {
        self.browser
            .new_page("about:blank")
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to create page: {e}")))
    }

    pub async fn pages(&self) -> crate::Result<Vec<chromiumoxide::Page>> {
        self.browser
            .pages()
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to list pages: {e}")))
    }

    pub async fn close(mut self) -> crate::Result<()> {
        self.browser
            .close()
            .await
            .map(|_| ())
            .map_err(|e| ImauthError::Browser(format!("Failed to close browser: {e}")))
    }
}

pub async fn navigate(
    page: &Page,
    url: &str,
    _timeout_secs: u64,
) -> crate::Result<()> {
    page.goto(url)
        .await
        .map_err(|e| ImauthError::Browser(format!("Navigation failed: {e}")))?;
    // Wait for domcontentloaded
    tokio::time::sleep(Duration::from_secs(2)).await;
    Ok(())
}

pub async fn get_page_text(page: &Page) -> crate::Result<String> {
    let text: String = page
        .evaluate("() => document.body.innerText")
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to get page text: {e}")))?
        .into_value()
        .map_err(|e| ImauthError::Browser(format!("Failed to parse page text: {e}")))?;
    Ok(text)
}

pub async fn fill_input(
    page: &Page,
    selector: &str,
    value: &str,
) -> crate::Result<()> {
    page.find_element(selector)
        .await
        .map_err(|e| ImauthError::Browser(format!("Element not found {selector}: {e}")))?
        .click()
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to click {selector}: {e}")))?;
    page.evaluate(format!(r#"() => {{
        const el = document.querySelector('{}');
        if (el) el.value = '{}';
    }}"#, selector.replace('"', "\\\""), value.replace('"', "\\\"")))
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to fill {selector}: {e}")))?;
    Ok(())
}

pub async fn press_enter(
    page: &Page,
    selector: &str,
) -> crate::Result<()> {
    page.evaluate(format!(
        r#"() => {{
            const el = document.querySelector('{}');
            if (el) {{
                const ev = new KeyboardEvent('keydown', {{ key: 'Enter', keyCode: 13 }});
                el.dispatchEvent(ev);
            }}
        }}"#,
        selector.replace('"', "\\\"")
    ))
    .await
    .map_err(|e| ImauthError::Browser(format!("Failed to press Enter: {e}")))?;
    Ok(())
}

pub async fn screenshot(_page: &Page) -> crate::Result<Vec<u8>> {
    // Screenshot implementation deferred — chromiumoxide API version mismatch
    Ok(vec![])
}

pub async fn get_cookies(
    page: &Page,
) -> crate::Result<Vec<crate::session::state::Cookie>> {
    let cookies = page
        .get_cookies()
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to get cookies: {e}")))?;

    Ok(cookies
        .into_iter()
        .map(|c| crate::session::state::Cookie {
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
