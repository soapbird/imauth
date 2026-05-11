use async_trait::async_trait;

/// Best-effort sink for HTML + screenshot snapshots taken during auth flows.
/// Implementations log errors via `tracing` rather than surfacing them — matching
/// the original behavior of `SnapshotCtx`.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SnapshotSink: Send + Sync {
    async fn capture<'a>(
        &'a self,
        session_id: &'a str,
        label: &'a str,
        html: &'a str,
        png: Option<&'a [u8]>,
    );
}
