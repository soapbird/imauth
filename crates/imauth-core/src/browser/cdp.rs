use crate::ImauthError;
use chromiumoxide::page::Page;
use futures::StreamExt;
use std::time::Duration;

fn js_string(value: &str) -> crate::Result<String> {
    serde_json::to_string(value)
        .map_err(|e| ImauthError::Browser(format!("Failed to encode JavaScript string: {e}")))
}

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

    pub async fn new_page(&self) -> crate::Result<chromiumoxide::Page> {
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

pub async fn navigate(page: &Page, url: &str, _timeout_secs: u64) -> crate::Result<()> {
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

pub async fn fill_input(page: &Page, selector: &str, value: &str) -> crate::Result<()> {
    let selector_js = js_string(selector)?;
    let value_js = js_string(value)?;

    let js = format!(
        r#"() => {{
            const el = document.querySelector({selector_js});
            if (!el) return false;
            el.scrollIntoView({{ block: 'center', inline: 'center' }});
            el.focus();
            const valueSetter = Object.getOwnPropertyDescriptor(el.__proto__, 'value')?.set;
            const prototype = Object.getPrototypeOf(el);
            const prototypeValueSetter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
            if (prototypeValueSetter && valueSetter !== prototypeValueSetter) {{
                prototypeValueSetter.call(el, {value_js});
            }} else if (valueSetter) {{
                valueSetter.call(el, {value_js});
            }} else {{
                el.value = {value_js};
            }}
            el.dispatchEvent(new Event('input', {{ bubbles: true }}));
            el.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return true;
        }}"#
    );
    let filled: bool = page
        .evaluate(js)
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to fill {selector}: {e}")))?
        .into_value()
        .map_err(|e| ImauthError::Browser(format!("Failed to parse fill result: {e}")))?;
    if !filled {
        return Err(ImauthError::Browser(format!(
            "Element not found {selector}"
        )));
    }
    Ok(())
}

pub async fn click_element(page: &Page, selector: &str) -> crate::Result<()> {
    page.find_element(selector)
        .await
        .map_err(|e| ImauthError::Browser(format!("Element not found {selector}: {e}")))?
        .click()
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to click {selector}: {e}")))?;
    Ok(())
}

pub async fn click_element_text(
    page: &Page,
    selector: &str,
    text_patterns: &[&str],
) -> crate::Result<bool> {
    let selector_js = js_string(selector)?;
    let patterns_js = serde_json::to_string(text_patterns)
        .map_err(|e| ImauthError::Browser(format!("Failed to encode text patterns: {e}")))?;

    let js = format!(
        r#"() => {{
            const patterns = {patterns_js}.map((pattern) => pattern.toLowerCase());
            const elements = Array.from(document.querySelectorAll({selector_js}));
            const target = elements.find((el) => {{
                const text = `${{el.innerText || ''}} ${{el.textContent || ''}} ${{el.value || ''}} ${{el.getAttribute('aria-label') || ''}}`.toLowerCase();
                return patterns.some((pattern) => text.includes(pattern));
            }});
            if (!target) return false;
            target.scrollIntoView({{ block: 'center', inline: 'center' }});
            target.click();
            return true;
        }}"#
    );

    page.evaluate(js)
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to click text element: {e}")))?
        .into_value()
        .map_err(|e| ImauthError::Browser(format!("Failed to parse click result: {e}")))
}

pub async fn press_enter(page: &Page, selector: &str) -> crate::Result<()> {
    let selector_js = js_string(selector)?;

    page.evaluate(format!(
        r#"() => {{
            const el = document.querySelector({selector_js});
            if (el) {{
                const ev = new KeyboardEvent('keydown', {{ key: 'Enter', keyCode: 13 }});
                el.dispatchEvent(ev);
            }}
        }}"#
    ))
    .await
    .map_err(|e| ImauthError::Browser(format!("Failed to press Enter: {e}")))?;
    Ok(())
}

pub async fn take_screenshot(page: &Page) -> crate::Result<Vec<u8>> {
    let params = chromiumoxide::page::ScreenshotParams::builder().build();
    page.screenshot(params)
        .await
        .map_err(|e| ImauthError::Browser(format!("Screenshot failed: {e}")))
}

pub async fn get_page_html(page: &Page) -> crate::Result<String> {
    let html: String = page
        .evaluate("() => document.documentElement.outerHTML")
        .await
        .map_err(|e| ImauthError::Browser(format!("Failed to get page HTML: {e}")))?
        .into_value()
        .map_err(|e| ImauthError::Browser(format!("Failed to parse page HTML: {e}")))?;
    Ok(html)
}

pub async fn get_cookies(page: &Page) -> crate::Result<Vec<crate::session::state::Cookie>> {
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

#[cfg(test)]
mod tests {
    use super::js_string;

    #[test]
    fn js_string_encodes_css_selectors_with_quotes() {
        assert_eq!(
            js_string("input[name='email']").unwrap(),
            r#""input[name='email']""#
        );
    }

    #[test]
    fn js_string_escapes_credential_characters() {
        assert_eq!(
            js_string(r#"pa'ss"word\test"#).unwrap(),
            r#""pa'ss\"word\\test""#
        );
    }
}
