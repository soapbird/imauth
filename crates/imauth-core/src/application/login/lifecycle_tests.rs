use super::*;
use crate::ports::browser::PageDriver;
use crate::ports::repository::{MockCookieRepository, MockSessionRepository};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Notify;

#[derive(Clone, Copy)]
enum BlockAt {
    Creation,
    Navigation,
    Cookies,
    Disconnect,
}

struct Probe {
    stage: BlockAt,
    entered: Notify,
    disconnect: Notify,
    recovered: AtomicBool,
    acquired: AtomicUsize,
    pages: AtomicUsize,
    navigated: AtomicUsize,
    closed: AtomicUsize,
}

struct Browser(Arc<Probe>);
struct Page(Arc<Probe>);

#[async_trait]
impl BrowserSessionFactory for Browser {
    async fn acquire(&self) -> crate::Result<Box<dyn BrowserSession>> {
        self.0.acquired.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Self(self.0.clone())))
    }
    fn viewer_url(&self) -> Option<String> {
        Some("http://viewer.test/held-slot".into())
    }
}

#[async_trait]
impl BrowserSession for Browser {
    async fn new_page(&mut self) -> crate::Result<Box<dyn PageDriver>> {
        self.0.pages.fetch_add(1, Ordering::SeqCst);
        if matches!(self.0.stage, BlockAt::Creation) {
            self.0.entered.notify_one();
            return std::future::pending().await;
        }
        Ok(Box::new(Page(self.0.clone())))
    }
    async fn reconnect(&mut self) -> crate::Result<Box<dyn PageDriver>> {
        self.0.recovered.store(true, Ordering::SeqCst);
        Ok(Box::new(Page(self.0.clone())))
    }
    async fn wait_disconnected(&self) {
        self.0.disconnect.notified().await;
    }
    async fn close(&mut self) -> crate::Result<()> {
        self.0.closed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn existing_pages(&self) -> crate::Result<Vec<Box<dyn PageDriver>>> {
        Ok(vec![])
    }
    fn viewer_url(&self) -> String {
        "http://viewer.test/held-slot".into()
    }
}

#[async_trait]
impl PageDriver for Page {
    async fn navigate(&self, _url: &str, timeout: u64) -> crate::Result<()> {
        assert_eq!(timeout, 7, "configured page timeout must reach the adapter");
        self.0.navigated.fetch_add(1, Ordering::SeqCst);
        if matches!(self.0.stage, BlockAt::Navigation) {
            self.0.entered.notify_one();
            std::future::pending().await
        } else {
            Ok(())
        }
    }
    async fn get_cookies(&self) -> crate::Result<Vec<Cookie>> {
        if matches!(self.0.stage, BlockAt::Cookies) {
            self.0.entered.notify_one();
            return std::future::pending().await;
        }
        if self.0.recovered.load(Ordering::SeqCst) {
            Ok(vec![Cookie {
                name: "sessionid".into(),
                value: "synthetic-session".into(),
                domain: ".instagram.com".into(),
                path: "/".into(),
                expires: None,
                http_only: true,
                secure: true,
            }])
        } else {
            Ok(vec![])
        }
    }
    async fn close(&self) -> crate::Result<()> {
        panic!("session owns target cleanup")
    }
    async fn screenshot(&self) -> crate::Result<Vec<u8>> {
        Ok(vec![])
    }
    async fn content_html(&self) -> crate::Result<String> {
        Ok(String::new())
    }
}

fn setup(stage: BlockAt) -> (LoginUseCase, Arc<Probe>) {
    let probe = Arc::new(Probe {
        stage,
        entered: Notify::new(),
        disconnect: Notify::new(),
        recovered: AtomicBool::new(false),
        acquired: AtomicUsize::new(0),
        pages: AtomicUsize::new(0),
        navigated: AtomicUsize::new(0),
        closed: AtomicUsize::new(0),
    });
    let mut sessions = MockSessionRepository::new();
    sessions.expect_create().returning(Ok);
    sessions.expect_update().returning(|_| Ok(()));
    sessions
        .expect_get()
        .returning(|_| Ok(Some(Session::new("active".into(), "instagram".into()))));
    let mut cookies = MockCookieRepository::new();
    if matches!(stage, BlockAt::Disconnect) {
        cookies
            .expect_save_login()
            .times(1)
            .returning(|_, _| Ok(()));
    } else {
        cookies.expect_save_login().times(0);
    }
    let uc = LoginUseCase::new(
        Arc::new(sessions),
        Arc::new(cookies),
        Arc::new(Browser(probe.clone())),
        Duration::from_secs(3),
    )
    .with_browser_timeouts(Duration::from_secs(11), Duration::from_secs(7));
    (uc, probe)
}

#[tokio::test(start_paused = true)]
async fn disconnection_cancels_inflight_navigation_and_cookie_reads() {
    for stage in [BlockAt::Creation, BlockAt::Navigation, BlockAt::Cookies] {
        let (uc, probe) = setup(stage);
        let (tx, rx) = mpsc::channel(8);
        let task = tokio::spawn(async move { uc.execute(Platform::Instagram, tx).await });
        probe.entered.notified().await;
        drop(rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("prompt cancellation")
            .unwrap();
        assert_eq!(probe.closed.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn preparation_and_user_budgets_bound_stalled_operations() {
    for (stage, expected, phase) in [
        (BlockAt::Creation, Duration::from_secs(7), "page creation"),
        (
            BlockAt::Navigation,
            Duration::from_secs(11),
            "browser preparation",
        ),
        (BlockAt::Cookies, Duration::from_secs(3), "user login"),
    ] {
        let (uc, probe) = setup(stage);
        let (tx, mut rx) = mpsc::channel(8);
        let start = Instant::now();
        uc.execute(Platform::Instagram, tx).await;
        assert_eq!(start.elapsed(), expected);
        let mut terminal = None;
        while let Some(event) = rx.recv().await {
            if let LoginEvent::Final(session, cookies) = event {
                assert!(cookies.is_empty());
                terminal = Some(session);
            }
        }
        let terminal = terminal.expect("terminal event");
        assert_eq!(terminal.state, SessionState::Failed);
        assert!(terminal.message.unwrap().contains(phase));
        assert_eq!(probe.closed.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn handler_disconnect_reattaches_without_reacquiring_or_navigating() {
    let (uc, probe) = setup(BlockAt::Disconnect);
    let (tx, mut rx) = mpsc::channel(8);
    let task = tokio::spawn(async move { uc.execute(Platform::Instagram, tx).await });
    assert!(matches!(rx.recv().await, Some(LoginEvent::Started(_))));
    let Some(LoginEvent::WaitingForUser(_, viewer)) = rx.recv().await else {
        panic!("waiting event")
    };
    assert_eq!(viewer, "http://viewer.test/held-slot");
    probe.disconnect.notify_one();
    let Some(LoginEvent::Final(session, cookies)) = rx.recv().await else {
        panic!("terminal event")
    };
    assert_eq!(session.state, SessionState::Connected);
    assert_eq!(cookies.len(), 1);
    task.await.unwrap();
    assert!(probe.recovered.load(Ordering::SeqCst));
    assert_eq!(probe.acquired.load(Ordering::SeqCst), 1);
    assert_eq!(probe.pages.load(Ordering::SeqCst), 1);
    assert_eq!(probe.navigated.load(Ordering::SeqCst), 1);
    assert_eq!(probe.closed.load(Ordering::SeqCst), 1);
}
