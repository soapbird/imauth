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

        let html_fut = tokio::fs::write(html_path, html);
        match png {
            Some(png) => {
                let (html_res, png_res) =
                    tokio::join!(html_fut, tokio::fs::write(png_path, png));
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
