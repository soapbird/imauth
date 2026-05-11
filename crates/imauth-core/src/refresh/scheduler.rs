use crate::platform::Platform;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::interval;

pub struct RefreshScheduler {
    platforms: HashMap<Platform, Duration>,
}

impl RefreshScheduler {
    pub fn new() -> Self {
        Self {
            platforms: HashMap::new(),
        }
    }

    pub fn add_platform(
        &mut self,
        platform: Platform,
        interval_secs: u64,
    ) {
        self.platforms
            .insert(platform, Duration::from_secs(interval_secs));
    }

    pub fn get_interval(&self,
        platform: Platform,
    ) -> Option<Duration> {
        self.platforms.get(&platform).copied()
    }

    pub fn platforms(&self) -> &HashMap<Platform, Duration> {
        &self.platforms
    }
}

/// Run a refresh loop for a single platform.
pub async fn run_refresh_loop(
    platform: Platform,
    duration: Duration,
    mut callback: impl FnMut(Platform) -> futures::future::BoxFuture<'static, crate::Result<()>> + Send + 'static,
) {
    let mut ticker = interval(duration);
    loop {
        ticker.tick().await;
        if let Err(e) = callback(platform).await {
            tracing::warn!("Refresh failed for {}: {}", platform.as_str(), e);
        }
    }
}
