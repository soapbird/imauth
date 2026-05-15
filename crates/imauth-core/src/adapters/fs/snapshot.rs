use crate::ports::snapshot::SnapshotSink;
use async_trait::async_trait;
use std::path::PathBuf;

pub struct FsSnapshotSink {
    base_dir: PathBuf,
}

impl FsSnapshotSink {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

fn redact_html_snapshot(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;

    while let Some(rel_start) = lower[pos..].find("<input") {
        let start = pos + rel_start;
        let Some(rel_end) = lower[start..].find('>') else {
            break;
        };
        let end = start + rel_end + 1;

        out.push_str(&html[pos..start]);
        out.push_str(&redact_value_attributes(&html[start..end]));
        pos = end;
    }

    out.push_str(&html[pos..]);
    out
}

fn redact_value_attributes(tag: &str) -> String {
    let lower = tag.to_ascii_lowercase();
    let mut out = String::with_capacity(tag.len());
    let mut pos = 0;

    while let Some(rel_value) = lower[pos..].find("value") {
        let value_start = pos + rel_value;
        let mut cursor = value_start + "value".len();

        while lower
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 1;
        }

        if lower.as_bytes().get(cursor) != Some(&b'=') {
            out.push_str(&tag[pos..cursor]);
            pos = cursor;
            continue;
        }

        cursor += 1;
        while lower
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 1;
        }

        let Some(&quote) = tag.as_bytes().get(cursor) else {
            break;
        };
        if quote != b'"' && quote != b'\'' {
            out.push_str(&tag[pos..cursor]);
            pos = cursor;
            continue;
        }

        let value_content_start = cursor + 1;
        let Some(rel_value_end) = tag[value_content_start..].find(quote as char) else {
            break;
        };
        let value_end = value_content_start + rel_value_end;

        out.push_str(&tag[pos..value_content_start]);
        out.push_str("[redacted]");
        pos = value_end;
    }

    out.push_str(&tag[pos..]);
    out
}

#[async_trait]
impl SnapshotSink for FsSnapshotSink {
    async fn capture<'a>(
        &'a self,
        session_id: &'a str,
        label: &'a str,
        html: &'a str,
        png: Option<&'a [u8]>,
    ) {
        let dir = self.base_dir.join(session_id);
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            tracing::warn!("Failed to create snapshot dir: {}", e);
            return;
        }

        let ts = chrono::Utc::now().timestamp_millis();
        let html_path = dir.join(format!("{label}_{ts}.html"));
        let png_path = dir.join(format!("{label}_{ts}.png"));

        let html = redact_html_snapshot(html);
        let html_fut = tokio::fs::write(html_path, html);
        match png {
            Some(png) => {
                let (html_res, png_res) = tokio::join!(html_fut, tokio::fs::write(png_path, png));
                if let Err(e) = html_res {
                    tracing::warn!("Failed to write HTML snapshot: {}", e);
                }
                if let Err(e) = png_res {
                    tracing::warn!("Failed to write screenshot: {}", e);
                }
            }
            None => {
                if let Err(e) = html_fut.await {
                    tracing::warn!("Failed to write HTML snapshot: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::redact_html_snapshot;

    #[test]
    fn redacts_login_input_values_from_snapshots() {
        let html = r#"<input name="email" value="user@example.com"><input type="password" value="secret">"#;

        let redacted = redact_html_snapshot(html);

        assert!(!redacted.contains("user@example.com"));
        assert!(!redacted.contains("secret"));
        assert_eq!(redacted.matches("[redacted]").count(), 2);
    }

    #[test]
    fn redacts_2fa_text_input_value_from_snapshots() {
        let html = r#"<div><input autocomplete="off" type="text" value="746736362"></div>"#;

        let redacted = redact_html_snapshot(html);

        assert!(!redacted.contains("746736362"));
        assert!(redacted.contains(r#"value="[redacted]""#));
    }

    #[test]
    fn redacts_single_quoted_input_values() {
        let html = r#"<input type='text' value='hunter2'>"#;
        let redacted = redact_html_snapshot(html);
        assert!(!redacted.contains("hunter2"));
        assert!(redacted.contains("[redacted]"));
    }

    #[test]
    fn redacts_uppercase_input_and_value_attributes() {
        let html = r#"<INPUT TYPE="password" VALUE="topsecret">"#;
        let redacted = redact_html_snapshot(html);
        assert!(!redacted.contains("topsecret"));
        assert!(redacted.contains("[redacted]"));
    }

    #[test]
    fn passes_through_html_with_no_inputs_unchanged() {
        let html = r#"<div><p>Hello</p><a href="/login">Sign in</a></div>"#;
        let redacted = redact_html_snapshot(html);
        assert_eq!(redacted, html);
    }

    #[test]
    fn leaves_non_value_attributes_on_inputs_untouched() {
        let html = r#"<input type="text" name="username" placeholder="username">"#;
        let redacted = redact_html_snapshot(html);
        // No `value="..."` attribute on this input, so nothing to redact.
        assert!(!redacted.contains("[redacted]"));
        // Other attributes preserved.
        assert!(redacted.contains(r#"name="username""#));
        assert!(redacted.contains(r#"placeholder="username""#));
    }
}
