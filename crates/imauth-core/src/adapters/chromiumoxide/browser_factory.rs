use crate::adapters::chromiumoxide::page_driver::ChromiumOxidePageDriver;
use crate::ports::browser::{BrowserSession, BrowserSessionFactory, PageDriver};
use crate::ImauthError;
use crate::Result;
use async_trait::async_trait;
use chromiumoxide::cdp::browser_protocol::target::{
    CloseTargetParams, CreateTargetParams, GetTargetsParams, TargetId,
};
use chromiumoxide::Browser;
use futures::{stream::FuturesUnordered, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;

const SESSION_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const TARGET_READY_RETRY_INTERVAL: Duration = Duration::from_millis(10);
const TARGET_READY_MAX_ATTEMPTS: u32 = 100;

/// A single Chrome instance with its own CDP connection, semaphore, and viewer URL.
pub struct ChromeSlot {
    cdp_url: String,
    semaphore: Arc<Semaphore>,
    viewer_url: String,
}

impl ChromeSlot {
    pub fn new(cdp_url: String, viewer_url: String) -> Self {
        Self {
            cdp_url,
            semaphore: Arc::new(Semaphore::new(1)), // 1 concurrent session per slot
            viewer_url,
        }
    }

    /// Chromium 131+ rejects CDP HTTP requests when the Host header contains
    /// a non-IP hostname (e.g. `chrome-0`). Resolve the hostname to an IP so
    /// the initial `json/version` request succeeds inside Docker networks.
    async fn resolve_cdp_url(cdp_url: &str) -> Result<String> {
        let mut url = url::Url::parse(cdp_url)
            .map_err(|e| ImauthError::Browser(format!("Invalid CDP URL: {e}")))?;
        let host = url
            .host_str()
            .ok_or_else(|| ImauthError::Browser("CDP URL has no host".into()))?;

        // If already an IP, nothing to do.
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(cdp_url.to_string());
        }

        let addr = tokio::net::lookup_host(format!("{}:{}", host, url.port().unwrap_or(9222)))
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to resolve {host}: {e}")))?
            .next()
            .ok_or_else(|| ImauthError::Browser(format!("No addresses for {host}")))?;

        url.set_host(Some(&addr.ip().to_string()))
            .map_err(|e| ImauthError::Browser(format!("Failed to set host: {e}")))?;
        Ok(url.to_string())
    }

    async fn connect(cdp_url: &str, target_id: Option<&TargetId>) -> Result<BrowserConnection> {
        let resolved = Self::resolve_cdp_url(cdp_url).await?;
        tracing::debug!("Connecting to CDP at {resolved} (original: {cdp_url})");
        let (browser, mut handler) = Browser::connect(&resolved)
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to connect to CDP: {e}")))?;
        let (disconnected_tx, disconnected_rx) = watch::channel(false);
        let handler_task = HandlerTask::new(tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if let Err(e) = h {
                    tracing::warn!("CDP handler stopped: {e}");
                    break;
                }
            }
            let _ = disconnected_tx.send(true);
        }));
        let targets = browser
            .execute(GetTargetsParams::default())
            .await
            .map_err(|error| {
                ImauthError::Browser(format!("Failed to discover CDP targets: {error}"))
            })?
            .result
            .target_infos;
        let (pending_targets, require_ready) = if let Some(target_id) = target_id {
            if !targets.iter().any(|target| &target.target_id == target_id) {
                return Err(ImauthError::Browser(
                    "Owned login target no longer exists".into(),
                ));
            }
            (vec![target_id.clone()], true)
        } else {
            (
                targets
                    .iter()
                    .filter(|target| target.r#type == "page")
                    .map(|target| target.target_id.clone())
                    .collect(),
                false,
            )
        };
        for target_id in pending_targets {
            let mut attempts = 0u32;
            loop {
                let ready = match browser.get_page(target_id.clone()).await {
                    Ok(page) => page.url().await.is_ok(),
                    Err(_) => false,
                };
                if ready {
                    break;
                }
                attempts += 1;
                if !require_ready && attempts >= TARGET_READY_MAX_ATTEMPTS {
                    // An unrelated tab that closed or never became ready must
                    // not hold the browser slot until the connect timeout.
                    break;
                }
                tokio::time::sleep(TARGET_READY_RETRY_INTERVAL).await;
            }
        }
        Ok(BrowserConnection {
            browser: Arc::new(browser),
            disconnected_rx,
            handler_task,
        })
    }
}

struct BrowserConnection {
    browser: Arc<Browser>,
    disconnected_rx: watch::Receiver<bool>,
    handler_task: HandlerTask,
}

impl BrowserConnection {
    async fn shutdown(mut self) {
        self.handler_task.shutdown().await;
    }
}

struct HandlerTask {
    task: Option<JoinHandle<()>>,
}

impl HandlerTask {
    fn new(task: JoinHandle<()>) -> Self {
        Self { task: Some(task) }
    }

    async fn shutdown(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for HandlerTask {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Pool of Chrome slots. Each slot is an independent Chrome instance with its
/// own viewer URL. Login acquires a free slot, login completion releases it.
pub struct PooledBrowserFactory {
    slots: Vec<ChromeSlot>,
    acquire_timeout: Duration,
    connect_timeout: Duration,
}

impl PooledBrowserFactory {
    pub fn new(
        cdp_urls: Vec<String>,
        viewer_urls: &[String],
        acquire_timeout: Duration,
        connect_timeout: Duration,
    ) -> Self {
        let slots = cdp_urls
            .into_iter()
            .enumerate()
            .map(|(i, cdp_url)| {
                let viewer_url = viewer_urls.get(i).cloned().unwrap_or_default();
                ChromeSlot::new(cdp_url, viewer_url)
            })
            .collect();

        Self {
            slots,
            acquire_timeout,
            connect_timeout,
        }
    }

    async fn connect(&self, slot: &ChromeSlot) -> Result<BrowserConnection> {
        tokio::time::timeout(
            self.connect_timeout,
            ChromeSlot::connect(&slot.cdp_url, None),
        )
        .await
        .map_err(|_| {
            ImauthError::Browser(format!(
                "CDP connection timed out after {}s",
                self.connect_timeout.as_secs()
            ))
        })?
    }

    async fn wait_for_slot(&self, slot_indices: &[usize]) -> Result<(usize, OwnedSemaphorePermit)> {
        let mut waiters = slot_indices
            .iter()
            .map(|&index| async move {
                (
                    index,
                    self.slots[index].semaphore.clone().acquire_owned().await,
                )
            })
            .collect::<FuturesUnordered<_>>();

        tokio::time::timeout(self.acquire_timeout, async {
            while let Some((index, permit)) = waiters.next().await {
                if let Ok(permit) = permit {
                    return Ok((index, permit));
                }
            }
            Err(ImauthError::Browser("All browser slots are closed".into()))
        })
        .await
        .map_err(|_| {
            ImauthError::Browser(format!(
                "Browser slot acquisition timed out after {}s",
                self.acquire_timeout.as_secs()
            ))
        })?
    }

    fn session(
        &self,
        slot_index: usize,
        permit: OwnedSemaphorePermit,
        connection: BrowserConnection,
    ) -> Box<dyn BrowserSession> {
        let slot = &self.slots[slot_index];
        Box::new(ChromiumOxideBrowserSession {
            connection: Some(connection),
            cdp_url: slot.cdp_url.clone(),
            connect_timeout: self.connect_timeout,
            viewer_url: slot.viewer_url.clone(),
            permit: Some(permit),
            target_id: None,
            page_creation: None,
        })
    }
}

#[async_trait]
impl BrowserSessionFactory for PooledBrowserFactory {
    async fn acquire(&self) -> Result<Box<dyn BrowserSession>> {
        let mut connect_errors = Vec::new();
        let mut busy_slots = Vec::new();
        for (slot_index, slot) in self.slots.iter().enumerate() {
            let Ok(permit) = slot.semaphore.clone().try_acquire_owned() else {
                busy_slots.push(slot_index);
                continue;
            };
            let connection = match self.connect(slot).await {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::warn!(cdp_url = %slot.cdp_url, %error, "failed to connect to browser slot");
                    connect_errors.push(error.to_string());
                    continue;
                }
            };
            return Ok(self.session(slot_index, permit, connection));
        }

        if self.slots.is_empty() {
            return Err(ImauthError::Browser("No browser slots configured".into()));
        }
        if !busy_slots.is_empty() {
            let (slot_index, permit) = self.wait_for_slot(&busy_slots).await?;
            let connection = self.connect(&self.slots[slot_index]).await?;
            return Ok(self.session(slot_index, permit, connection));
        }
        if !connect_errors.is_empty() {
            return Err(ImauthError::Browser(format!(
                "Failed to connect to available browser slots: {}",
                connect_errors.join("; ")
            )));
        }

        Err(ImauthError::Browser(
            "No available browser slots could be acquired".into(),
        ))
    }

    fn viewer_url(&self) -> Option<String> {
        // Return the first slot's URL as default; actual per-slot URL
        // is returned via the session's viewer_url field.
        self.slots.first().map(|s| s.viewer_url.clone())
    }
}

/// A held browser connection for a single login attempt (RAII).
pub struct ChromiumOxideBrowserSession {
    connection: Option<BrowserConnection>,
    cdp_url: String,
    connect_timeout: Duration,
    viewer_url: String,
    permit: Option<OwnedSemaphorePermit>,
    target_id: Option<TargetId>,
    page_creation: Option<JoinHandle<chromiumoxide::error::Result<TargetId>>>,
}

impl ChromiumOxideBrowserSession {
    fn connection(&self) -> Result<&BrowserConnection> {
        self.connection
            .as_ref()
            .ok_or_else(|| ImauthError::Browser("browser session already released".into()))
    }

    pub fn viewer_url(&self) -> &str {
        &self.viewer_url
    }

    #[cfg(test)]
    async fn evaluate_owned_page(&self, expression: &str) -> Result<()> {
        let target_id = self
            .target_id
            .clone()
            .ok_or_else(|| ImauthError::Browser("browser session has no login target".into()))?;
        let page = self
            .connection()?
            .browser
            .get_page(target_id)
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?;
        page.evaluate(expression)
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?;
        Ok(())
    }

    async fn finish_page_creation(&mut self) -> Result<TargetId> {
        let joined = self
            .page_creation
            .as_mut()
            .ok_or_else(|| ImauthError::Browser("page creation is not running".into()))?
            .await;
        self.page_creation.take();
        let result = joined
            .map_err(|error| ImauthError::Browser(format!("Page creation task failed: {error}")))?;
        result.map_err(|error| ImauthError::Browser(format!("Failed to create page: {error}")))
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Spawn cleanup detached so a caller-side timeout (login.rs wraps
        // close() in CLEANUP_TIMEOUT) cannot strand an owned login target:
        // the task finishes closing the tab even when the await is dropped.
        let cleanup = tokio::spawn(cleanup_session(
            self.connection.take(),
            self.target_id.take(),
            self.page_creation.take(),
            self.permit.take(),
            self.cdp_url.clone(),
            self.connect_timeout,
        ));
        cleanup.await.map_err(|error| {
            ImauthError::Browser(format!("Browser cleanup task failed: {error}"))
        })?
    }
}

async fn cleanup_session(
    connection: Option<BrowserConnection>,
    target_id: Option<TargetId>,
    page_creation: Option<JoinHandle<chromiumoxide::error::Result<TargetId>>>,
    _permit: Option<OwnedSemaphorePermit>,
    cdp_url: String,
    connect_timeout: Duration,
) -> Result<()> {
    let pending_target = match page_creation {
        Some(mut task) => match tokio::time::timeout(SESSION_CLOSE_TIMEOUT, &mut task).await {
            Ok(Ok(Ok(target_id))) => Some(target_id),
            Ok(Ok(Err(error))) => {
                tracing::warn!(%error, "page creation failed during session cleanup");
                None
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "page creation task failed during session cleanup");
                None
            }
            Err(_) => {
                task.abort();
                let _ = task.await;
                None
            }
        },
        None => None,
    };

    let owned_target = target_id.or(pending_target);

    // A failed reconnect leaves an owned target with no connection. Open a
    // throwaway connection solely to close it, or the login tab stays alive
    // in the shared browser and is visible to the next slot holder.
    let mut connection = connection;
    if connection.is_none() && owned_target.is_some() {
        match tokio::time::timeout(
            connect_timeout,
            ChromeSlot::connect(&cdp_url, owned_target.as_ref()),
        )
        .await
        {
            Ok(Ok(fresh)) => connection = Some(fresh),
            Ok(Err(error)) => {
                tracing::warn!(%error, "reconnect-to-close failed during session cleanup")
            }
            Err(_) => tracing::warn!("reconnect-to-close timed out during session cleanup"),
        }
    }

    let close_result = if let (Some(connection), Some(target_id)) = (&connection, owned_target) {
        tokio::time::timeout(
            SESSION_CLOSE_TIMEOUT,
            connection
                .browser
                .execute(CloseTargetParams::new(target_id)),
        )
        .await
        .map(|result| result.map(|_| ()))
    } else {
        Ok(Ok(()))
    };

    if let Some(connection) = connection {
        connection.shutdown().await;
    }

    close_result
        .map_err(|_| ImauthError::Browser("Browser target close timed out".into()))?
        .map_err(|error| ImauthError::Browser(format!("Failed to close browser target: {error}")))
}

#[async_trait]
impl BrowserSession for ChromiumOxideBrowserSession {
    async fn new_page(&mut self) -> Result<Box<dyn PageDriver>> {
        if self.target_id.is_some() || self.page_creation.is_some() {
            return Err(ImauthError::Browser(
                "browser session already owns a login target".into(),
            ));
        }
        let connection = self.connection()?;
        let browser = Arc::clone(&connection.browser);
        let creation_browser = Arc::clone(&browser);
        let disconnected = connection.disconnected_rx.clone();
        self.page_creation = Some(tokio::spawn(async move {
            creation_browser
                .execute(CreateTargetParams::new("about:blank"))
                .await
                .map(|response| response.result.target_id)
        }));
        let target_id = self.finish_page_creation().await?;
        self.target_id = Some(target_id.clone());
        let page = loop {
            if let Ok(page) = browser.get_page(target_id.clone()).await {
                if page.url().await.is_ok() {
                    break page;
                }
            }
            tokio::time::sleep(TARGET_READY_RETRY_INTERVAL).await;
        };
        Ok(Box::new(ChromiumOxidePageDriver::new(page, disconnected)))
    }

    async fn reconnect(&mut self) -> Result<Box<dyn PageDriver>> {
        let target_id = self
            .target_id
            .clone()
            .ok_or_else(|| ImauthError::Browser("browser session has no login target".into()))?;
        if let Some(connection) = self.connection.take() {
            connection.shutdown().await;
        }
        let connection = tokio::time::timeout(
            self.connect_timeout,
            ChromeSlot::connect(&self.cdp_url, Some(&target_id)),
        )
        .await
        .map_err(|_| ImauthError::Browser("CDP reconnect timed out".into()))??;
        let page = connection
            .browser
            .get_page(target_id)
            .await
            .map_err(|error| {
                ImauthError::Browser(format!("Failed to reattach login target: {error}"))
            })?;
        let disconnected = connection.disconnected_rx.clone();
        self.connection = Some(connection);
        Ok(Box::new(ChromiumOxidePageDriver::new(page, disconnected)))
    }

    async fn wait_disconnected(&self) {
        let Ok(connection) = self.connection() else {
            return;
        };
        let mut receiver = connection.disconnected_rx.clone();
        if *receiver.borrow() {
            return;
        }
        let _ = receiver.changed().await;
    }

    async fn close(&mut self) -> Result<()> {
        self.shutdown().await
    }

    async fn existing_pages(&self) -> Result<Vec<Box<dyn PageDriver>>> {
        let connection = self.connection()?;
        let pages = connection
            .browser
            .pages()
            .await
            .map_err(|e| ImauthError::Browser(format!("Failed to list pages: {e}")))?;
        Ok(pages
            .into_iter()
            .map(|page| {
                Box::new(ChromiumOxidePageDriver::new(
                    page,
                    connection.disconnected_rx.clone(),
                )) as Box<dyn PageDriver>
            })
            .collect())
    }

    fn viewer_url(&self) -> String {
        self.viewer_url.clone()
    }
}

impl Drop for ChromiumOxideBrowserSession {
    fn drop(&mut self) {
        if self.connection.is_none() && self.page_creation.is_none() && self.target_id.is_none() {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            if let Some(task) = self.page_creation.take() {
                task.abort();
            }
            if let Some(connection) = self.connection.take() {
                drop(connection);
            }
            self.permit.take();
            return;
        };
        let connection = self.connection.take();
        let target_id = self.target_id.take();
        let page_creation = self.page_creation.take();
        let permit = self.permit.take();
        let cdp_url = self.cdp_url.clone();
        let connect_timeout = self.connect_timeout;
        handle.spawn(async move {
            if let Err(error) = cleanup_session(
                connection,
                target_id,
                page_creation,
                permit,
                cdp_url,
                connect_timeout,
            )
            .await
            {
                tracing::warn!(%error, "browser session cleanup failed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    #[test]
    fn explicit_viewer_urls_are_used_without_legacy_novnc_fallback() {
        let viewer_urls = vec![
            "http://localhost:6101/index.html".to_string(),
            "http://localhost:6102/index.html".to_string(),
            "http://localhost:6103/index.html".to_string(),
        ];
        let factory = PooledBrowserFactory::new(
            vec![
                "http://chrome-0:9223".to_string(),
                "http://chrome-1:9223".to_string(),
                "http://chrome-2:9223".to_string(),
            ],
            &viewer_urls,
            Duration::from_secs(30),
            Duration::from_secs(5),
        );

        assert_eq!(
            factory.slots[0].viewer_url,
            "http://localhost:6101/index.html"
        );
        assert_eq!(
            factory.slots[1].viewer_url,
            "http://localhost:6102/index.html"
        );
        assert_eq!(
            factory.slots[2].viewer_url,
            "http://localhost:6103/index.html"
        );
    }

    #[test]
    fn empty_viewer_urls_do_not_generate_legacy_novnc_urls() {
        let factory = PooledBrowserFactory::new(
            vec!["http://chrome-0:9223".to_string()],
            &[],
            Duration::from_secs(30),
            Duration::from_secs(5),
        );

        assert_eq!(factory.viewer_url().as_deref(), Some(""));
    }

    #[test]
    fn missing_viewer_urls_are_not_reused_for_later_slots() {
        let viewer_urls = vec!["http://localhost:6101/index.html".to_string()];
        let factory = PooledBrowserFactory::new(
            vec![
                "http://chrome-0:9223".to_string(),
                "http://chrome-1:9223".to_string(),
            ],
            &viewer_urls,
            Duration::from_secs(30),
            Duration::from_secs(5),
        );

        assert_eq!(factory.slots[0].viewer_url, viewer_urls[0]);
        assert!(factory.slots[1].viewer_url.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_times_out_when_every_slot_is_busy() {
        let factory = PooledBrowserFactory::new(
            vec!["http://chrome-0:9223".to_string()],
            &[],
            Duration::from_secs(1),
            Duration::from_secs(5),
        );
        let _permit = factory.slots[0]
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap();

        let result = factory.acquire().await;

        assert!(matches!(
            result,
            Err(ImauthError::Browser(message)) if message.contains("acquisition timed out")
        ));
    }

    #[tokio::test]
    async fn wait_for_slot_uses_the_first_slot_released() {
        // Given two busy browser slots.
        let factory = Arc::new(PooledBrowserFactory::new(
            vec![
                "http://chrome-0:9223".to_string(),
                "http://chrome-1:9223".to_string(),
            ],
            &[],
            Duration::from_secs(1),
            Duration::from_secs(5),
        ));
        let first = factory.slots[0]
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap();
        let second = factory.slots[1]
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap();

        let waiting_factory = Arc::clone(&factory);
        let waiter = tokio::spawn(async move { waiting_factory.wait_for_slot(&[0, 1]).await });
        tokio::task::yield_now().await;

        // When the non-first slot becomes available while acquisition is waiting.
        drop(second);
        let (slot_index, _permit) = waiter.await.unwrap().unwrap();

        // Then the waiter acquires that slot without waiting for slot zero.
        assert_eq!(slot_index, 1);
        drop(first);
    }

    #[tokio::test(start_paused = true)]
    async fn handler_is_aborted_when_target_discovery_times_out() {
        // Given connection setup owns a running handler through its RAII guard.
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let setup = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(1), async move {
                let task = tokio::spawn(async move {
                    let _drop_signal = DropSignal(Some(dropped_tx));
                    let _ = started_tx.send(());
                    std::future::pending::<()>().await;
                });
                let _handler = HandlerTask::new(task);
                std::future::pending::<()>().await;
            })
            .await
        });
        started_rx.await.unwrap();

        // When target discovery reaches its connection timeout.
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(setup.await.unwrap().is_err());

        // Then cancelling setup aborts and drops the handler task.
        dropped_rx.await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires an isolated real Chromium CDP endpoint"]
    async fn real_cdp_reconnect_preserves_owned_target_and_close_removes_it() -> Result<()> {
        // Given an isolated Chromium slot and one owned login target.
        let cdp_url = std::env::var("IMAUTH_TEST_CDP_URL")
            .map_err(|error| ImauthError::Config(format!("IMAUTH_TEST_CDP_URL: {error}")))?;
        let factory = PooledBrowserFactory::new(
            vec![cdp_url],
            &[],
            Duration::from_secs(5),
            Duration::from_secs(5),
        );
        let permit = factory.slots[0]
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?;
        let connection = factory.connect(&factory.slots[0]).await?;
        let baseline_pages = connection
            .browser
            .pages()
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?
            .len();
        let mut session = ChromiumOxideBrowserSession {
            connection: Some(connection),
            cdp_url: factory.slots[0].cdp_url.clone(),
            connect_timeout: Duration::from_secs(5),
            viewer_url: String::new(),
            permit: Some(permit),
            target_id: None,
            page_creation: None,
        };
        let page = session.new_page().await?;
        page.navigate("data:text/html,<input id='state' value='preserved'>", 5)
            .await?;
        session
            .evaluate_owned_page(
                "document.getElementById('state').setAttribute('value', 'typed-after-navigation')",
            )
            .await?;
        let target_before = session.target_id.clone();

        // When the CDP connection remains idle and then reconnects.
        tokio::time::sleep(Duration::from_secs(14)).await;
        let idle_dom_preserved = page
            .content_html()
            .await?
            .contains("value=\"typed-after-navigation\"");
        assert!(idle_dom_preserved);
        let reconnected_page = session.reconnect().await?;

        // Then the same target and DOM remain, and closing restores the page count.
        assert_eq!(session.target_id, target_before);
        let reconnected_dom_preserved = reconnected_page
            .content_html()
            .await?
            .contains("value=\"typed-after-navigation\"");
        assert!(reconnected_dom_preserved);
        assert_eq!(session.existing_pages().await?.len(), baseline_pages + 1);
        session.close().await?;
        let final_connection = factory.connect(&factory.slots[0]).await?;
        let final_pages = final_connection
            .browser
            .pages()
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?
            .len();
        assert_eq!(final_pages, baseline_pages);
        final_connection.shutdown().await;

        let permit = factory.slots[0]
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?;
        let connection = factory.connect(&factory.slots[0]).await?;
        let browser = Arc::clone(&connection.browser);
        let mut cancelled_session = ChromiumOxideBrowserSession {
            connection: Some(connection),
            cdp_url: factory.slots[0].cdp_url.clone(),
            connect_timeout: Duration::from_secs(5),
            viewer_url: String::new(),
            permit: Some(permit),
            target_id: None,
            page_creation: Some(tokio::spawn(async move {
                browser
                    .execute(CreateTargetParams::new("about:blank"))
                    .await
                    .map(|response| response.result.target_id)
            })),
        };
        tokio::task::yield_now().await;
        cancelled_session.close().await?;
        let cleanup_connection = factory.connect(&factory.slots[0]).await?;
        let cleanup_pages = cleanup_connection
            .browser
            .pages()
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?
            .len();
        assert_eq!(cleanup_pages, baseline_pages);
        cleanup_connection.shutdown().await;

        let permit = factory.slots[0]
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?;
        let connection = factory.connect(&factory.slots[0]).await?;
        let mut dropped_session = ChromiumOxideBrowserSession {
            connection: Some(connection),
            cdp_url: factory.slots[0].cdp_url.clone(),
            connect_timeout: Duration::from_secs(5),
            viewer_url: String::new(),
            permit: Some(permit),
            target_id: None,
            page_creation: None,
        };
        let dropped_page = dropped_session.new_page().await?;
        drop(dropped_page);
        drop(dropped_session);
        let permit_after_drop = tokio::time::timeout(
            Duration::from_secs(5),
            factory.slots[0].semaphore.clone().acquire_owned(),
        )
        .await
        .map_err(|_| ImauthError::Browser("Browser permit was not released on drop".into()))?
        .map_err(|error| ImauthError::Browser(error.to_string()))?;
        let drop_cleanup_connection = factory.connect(&factory.slots[0]).await?;
        let pages_after_drop = drop_cleanup_connection
            .browser
            .pages()
            .await
            .map_err(|error| ImauthError::Browser(error.to_string()))?
            .len();
        assert_eq!(pages_after_drop, baseline_pages);
        drop_cleanup_connection.shutdown().await;
        drop(permit_after_drop);

        println!(
            "baseline_pages={baseline_pages} target={target_before:?} idle_dom_preserved={idle_dom_preserved} reconnected_dom_preserved={reconnected_dom_preserved} pages_after_close={final_pages} pages_after_cancelled_new_page={cleanup_pages} pages_after_drop={pages_after_drop}"
        );
        Ok(())
    }
}
