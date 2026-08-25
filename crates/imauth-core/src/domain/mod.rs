//! Pure domain types and logic. No I/O, no async, no Arc<>.

pub mod auth;
pub mod credential;
pub mod platform;
pub mod session;

pub use credential::Credential;
pub use platform::Platform;
pub use session::{Cookie, Session, SessionState};
