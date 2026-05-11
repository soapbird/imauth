// Threads shares Instagram auth (same Meta account).
// We reuse Instagram cookies for Threads access.
// The login flow itself is identical since Threads uses Instagram/Meta SSO.

use crate::platform::instagram;
use crate::session::state::Session;
use chromiumoxide::page::Page;

pub async fn login(
    page: &Page,
    username: &str,
    password: &str,
    session: &mut Session,
) -> crate::Result<()> {
    // Threads uses the same auth system as Instagram
    instagram::login(page, username, password, session).await
}

pub async fn submit_2fa(
    page: &Page,
    code: &str,
    session: &mut Session,
) -> crate::Result<()> {
    instagram::submit_2fa(page, code, session).await
}
