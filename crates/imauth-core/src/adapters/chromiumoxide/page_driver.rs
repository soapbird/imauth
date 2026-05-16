use crate::domain::session::Cookie;
use crate::ports::browser::PageDriver;
use crate::ImauthError;
use crate::Result;
use async_trait::async_trait;
use chromiumoxide::cdp::browser_protocol::input::InsertTextParams;
use chromiumoxide::page::Page;
use serde::de::DeserializeOwned;
use std::time::Duration;

pub struct ChromiumOxidePageDriver {
    page: Page,
}

impl ChromiumOxidePageDriver {
    pub fn new(page: Page) -> Self {
        Self { page }
    }

    async fn eval_into<T: DeserializeOwned>(&self, js: impl Into<String>, ctx: &str) -> Result<T> {
        self.page
            .evaluate(js.into())
            .await
            .map_err(|e| ImauthError::Browser(format!("{ctx} eval failed: {e}")))?
            .into_value()
            .map_err(|e| ImauthError::Browser(format!("{ctx} result parse failed: {e}")))
    }
}

fn js_string(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|e| ImauthError::Browser(format!("Failed to encode JavaScript string: {e}")))
}

#[async_trait]
impl PageDriver for ChromiumOxidePageDriver {
    async fn navigate(&self, url: &str, timeout_secs: u64) -> Result<()> {
        let nav = async {
            self.page
                .goto(url)
                .await
                .map_err(|e| ImauthError::Browser(format!("Navigation failed: {e}")))?;
            self.page
                .wait_for_navigation()
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

    async fn find_element(&self, selector: &str) -> Result<bool> {
        let selector_js = js_string(selector)?;
        self.eval_into(
            format!("() => document.querySelector({selector_js}) !== null"),
            "find_element",
        )
        .await
    }

    async fn fill_input(&self, selector: &str, value: &str) -> Result<()> {
        let selector_js = js_string(selector)?;

        let exists: bool = self
            .eval_into(
                format!(
                    r#"() => {{
                        const el = document.querySelector({selector_js});
                        if (!el) return false;
                        el.scrollIntoView({{ block: 'center', inline: 'center' }});
                        el.focus();
                        return document.activeElement === el;
                    }}"#
                ),
                "focus_input",
            )
            .await?;
        if !exists {
            return Err(ImauthError::Browser(format!(
                "Could not focus input {selector}"
            )));
        }

        let cleared: bool = self
            .eval_into(
                format!(
                    r#"() => {{
                        const el = document.querySelector({selector_js});
                        if (!el) return false;
                        el.scrollIntoView({{ block: 'center', inline: 'center' }});
                        el.focus();

                        const prototype = Object.getPrototypeOf(el);
                        const valueSetter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
                        const previousValue = el.value;
                        if (valueSetter) {{
                            valueSetter.call(el, '');
                        }} else {{
                            el.value = '';
                        }}

                        // React tracks the last input value internally. Resetting the tracker
                        // before dispatching lets controlled inputs observe the clear event.
                        const tracker = el._valueTracker;
                        if (tracker) tracker.setValue(previousValue);
                        el.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'deleteContentBackward' }}));
                        el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                        return document.activeElement === el;
                    }}"#
                ),
                "clear_input",
            )
            .await?;
        if !cleared {
            return Err(ImauthError::Browser(format!(
                "Could not focus input {selector}"
            )));
        }

        self.page
            .execute(InsertTextParams::new(value))
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to type into {selector}: {e}")))?;

        let value_matches: bool = self
            .eval_into(
                format!(
                    r#"() => {{
                        const el = document.querySelector({selector_js});
                        return !!el && el.value.length > 0;
                    }}"#
                ),
                "verify_input_value",
            )
            .await?;
        if !value_matches {
            return Err(ImauthError::Browser(format!(
                "Input {selector} remained empty after typing"
            )));
        }

        Ok(())
    }

    async fn click_element(&self, selector: &str) -> Result<()> {
        self.page
            .find_element(selector)
            .await
            .map_err(|e| ImauthError::Browser(format!("Element not found {selector}: {e}")))?
            .click()
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to click {selector}: {e}")))?;
        Ok(())
    }

    async fn click_element_text(&self, selector: &str, texts: &[String]) -> Result<bool> {
        let selector_js = js_string(selector)?;
        let patterns_js = serde_json::to_string(texts)
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

        self.eval_into(js, "click_element_text").await
    }

    async fn press_enter(&self, selector: &str) -> Result<()> {
        let selector_js = js_string(selector)?;
        self.eval_into::<serde_json::Value>(
            format!(
                r#"() => {{
                    const el = document.querySelector({selector_js});
                    if (el) {{
                        const opts = {{ key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true }};
                        el.dispatchEvent(new KeyboardEvent('keydown', opts));
                        el.dispatchEvent(new KeyboardEvent('keypress', opts));
                        el.dispatchEvent(new KeyboardEvent('keyup', opts));
                    }}
                }}"#
            ),
            "press_enter",
        )
        .await?;
        Ok(())
    }

    async fn get_page_text(&self) -> Result<String> {
        self.eval_into("() => document.body.innerText", "get_page_text")
            .await
    }

    async fn get_cookies(&self) -> Result<Vec<Cookie>> {
        let cookies = self
            .page
            .get_cookies()
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to get cookies: {e}")))?;

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
        let params = chromiumoxide::page::ScreenshotParams::builder().build();
        self.page
            .screenshot(params)
            .await
            .map_err(|e| ImauthError::Browser(format!("Screenshot failed: {e}")))
    }

    async fn content_html(&self) -> Result<String> {
        self.eval_into("() => document.documentElement.outerHTML", "content_html")
            .await
    }
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
