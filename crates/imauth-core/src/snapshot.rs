use crate::browser::cdp::{get_page_html, take_screenshot};
use chromiumoxide::page::Page;
use std::path::Path;

pub struct SnapshotCtx<'a> {
    dir: Option<&'a Path>,
    seq: u32,
}

impl<'a> SnapshotCtx<'a> {
    pub fn new(dir: Option<&'a Path>) -> Self {
        Self { dir, seq: 1 }
    }

    pub async fn capture(&mut self, page: &Page, stage: &str) {
        let Some(dir) = self.dir else { return };

        let (html_res, shot_res) = tokio::join!(get_page_html(page), take_screenshot(page));
        let html = html_res.unwrap_or_default();
        let screenshot = match shot_res {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!("Screenshot failed for stage '{}': {}", stage, e);
                Vec::new()
            }
        };

        if let Err(e) = tokio::fs::create_dir_all(dir).await {
            tracing::warn!("Failed to create snapshot dir: {}", e);
            return;
        }

        let ts = chrono::Utc::now().timestamp();
        let base = format!("{:03}_{}_{}", self.seq, stage, ts);
        let html_path = dir.join(format!("{}.html", base));
        let png_path = dir.join(format!("{}.png", base));

        if screenshot.is_empty() {
            if let Err(e) = tokio::fs::write(&html_path, html).await {
                tracing::warn!("Failed to write HTML snapshot: {}", e);
            }
        } else {
            let (html_w, png_w) = tokio::join!(
                tokio::fs::write(&html_path, html),
                tokio::fs::write(&png_path, screenshot),
            );
            if let Err(e) = html_w {
                tracing::warn!("Failed to write HTML snapshot: {}", e);
            }
            if let Err(e) = png_w {
                tracing::warn!("Failed to write screenshot: {}", e);
            }
        }

        self.seq += 1;
    }
}
