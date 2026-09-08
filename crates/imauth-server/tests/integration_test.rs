#![allow(clippy::result_large_err)]

#[path = "integration/auth.rs"]
mod auth;
#[path = "integration/credential.rs"]
mod credential;
#[path = "integration/login_lifecycle.rs"]
mod login_lifecycle;
#[path = "integration/session.rs"]
mod session;
#[path = "integration/support.rs"]
mod support;
#[path = "integration/transport.rs"]
mod transport;
