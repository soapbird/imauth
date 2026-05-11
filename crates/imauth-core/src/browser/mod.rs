pub mod cdp;
pub mod pool;

use pool::{BrowserHandle, BrowserPool};

pub struct BrowserManager {
    pool: BrowserPool,
}

impl BrowserManager {
    pub fn new(cdp_url: String, max_pool_size: usize) -> Self {
        Self {
            pool: BrowserPool::new(cdp_url, max_pool_size),
        }
    }

    pub async fn acquire(&self) -> crate::Result<BrowserHandle> {
        self.pool.acquire().await
    }
}
