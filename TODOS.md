# TODOS

## Browser / Login Lifecycle

### Un-ignore the real-CDP reconnect test in CI

**Priority:** P1

`real_cdp_reconnect_preserves_owned_target_and_close_removes_it` in
`crates/imauth-core/src/adapters/chromiumoxide/browser_factory.rs` is marked
`#[ignore = "requires an isolated real Chromium CDP endpoint"]`, so the reconnect
and target-cleanup behavior — the code path behind the 2026-09-08 pre-landing
fixes (reconnect-to-close, detached cleanup) — has no CI gate. Add a Chromium
service to the CI workflow and run this test there, or split the CDP operations
behind a fakeable adapter so the behavior is covered by deterministic tests.

**Noticed:** 2026-09-08 /ship pre-landing review on `develop`.

## Completed
