//! In-process registry binding session_id to the exact browser + page used
//! during Login. Submit2Fa must drive that same page; without this binding,
//! two concurrent logins on different sessions could end up typing their 2FA
//! codes into each other's tab because the browser pool returns *any*
//! healthy browser, not the one that ran Login.

use crate::domain::Platform;
use crate::ports::browser::{BrowserSession, PageDriver};
use std::collections::HashMap;
use tokio::sync::Mutex;

/// Holds the browser + page bound to an in-flight session. Dropping this
/// returns the browser to the pool (via BrowserSession::Drop) and closes the
/// page.
pub struct ActiveLoginSession {
    /// Kept alive so the page stays attached to the right CDP target.
    pub browser: Box<dyn BrowserSession>,
    pub page: Box<dyn PageDriver>,
    pub platform: Platform,
}

/// Registry of session_id -> active browser+page. The map is held behind a
/// `tokio::sync::Mutex` so callers can `await` while operating on a removed
/// entry without blocking other sessions from registering/removing in
/// parallel — entries are owned by value, never borrowed.
#[derive(Default)]
pub struct ActiveSessionRegistry {
    inner: Mutex<HashMap<String, ActiveLoginSession>>,
}

impl ActiveSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a session_id to its browser+page. Replaces any prior entry
    /// (which should never happen in practice; UUID collisions are
    /// vanishingly unlikely, but we don't want to leak the prior one).
    pub async fn register(&self, session_id: String, session: ActiveLoginSession) {
        let mut guard = self.inner.lock().await;
        guard.insert(session_id, session);
    }

    /// Take the browser+page bound to a session and remove the mapping. The
    /// caller owns the result and is responsible for dropping it (which
    /// returns the browser to the pool). Submit2Fa uses this to operate on
    /// the same page that Login left in a 2FA-required state.
    pub async fn take(&self, session_id: &str) -> Option<ActiveLoginSession> {
        let mut guard = self.inner.lock().await;
        guard.remove(session_id)
    }

    /// Drop a session's binding without surfacing the value (e.g. when the
    /// gRPC client cancels mid-stream). Quietly returns false if no entry
    /// existed.
    pub async fn discard(&self, session_id: &str) -> bool {
        let mut guard = self.inner.lock().await;
        guard.remove(session_id).is_some()
    }

    #[cfg(test)]
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::browser::{MockBrowserSession, MockPageDriver};

    fn make_active(platform: Platform) -> ActiveLoginSession {
        ActiveLoginSession {
            browser: Box::new(MockBrowserSession::new()),
            page: Box::new(MockPageDriver::new()),
            platform,
        }
    }

    #[tokio::test]
    async fn register_then_take_returns_same_entry_and_clears_map() {
        let reg = ActiveSessionRegistry::new();
        reg.register("s1".into(), make_active(Platform::Instagram))
            .await;
        assert_eq!(reg.len().await, 1);
        let taken = reg.take("s1").await;
        assert!(taken.is_some(), "registered session must be retrievable");
        assert_eq!(reg.len().await, 0, "take must remove the entry");
    }

    #[tokio::test]
    async fn take_unknown_returns_none() {
        let reg = ActiveSessionRegistry::new();
        assert!(reg.take("missing").await.is_none());
    }

    #[tokio::test]
    async fn register_replaces_prior_entry() {
        let reg = ActiveSessionRegistry::new();
        reg.register("s1".into(), make_active(Platform::Instagram))
            .await;
        reg.register("s1".into(), make_active(Platform::Threads))
            .await;
        assert_eq!(reg.len().await, 1);
        let taken = reg.take("s1").await.expect("entry exists");
        assert_eq!(taken.platform, Platform::Threads);
    }

    #[tokio::test]
    async fn discard_removes_silently() {
        let reg = ActiveSessionRegistry::new();
        reg.register("s1".into(), make_active(Platform::Instagram))
            .await;
        assert!(reg.discard("s1").await);
        assert_eq!(reg.len().await, 0);
        assert!(!reg.discard("s1").await, "second discard is a no-op");
    }
}
