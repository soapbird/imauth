# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0] - 2026-06-05

### Added
- User-driven login model: the server opens a browser via noVNC and returns a `viewer_url` so operators can log in themselves. Replaces the previous automated credential-stuffing approach with a more reliable human-in-the-loop flow.
- Naver platform support: full login URL, cookie domains (`.naver.com`, `.nid.naver.com`), session cookie (`NID_AUT`), and domain-scoped cookie filtering.
- PostgreSQL storage backend: `PostgresCookieRepository`, `PostgresCredentialRepository`, `PostgresSessionRepository`, and `PostgresRefreshTokenRepository` with parameterized queries, ON CONFLICT upserts, and automatic schema migrations. Configure via `IMAUTH_DATABASE_URL`; falls back to SQLite when unset.
- Environment-only configuration: `Config::from_env()` reads all settings from `IMAUTH_*` env vars. Removed TOML config dependency — operators configure with `.env` or environment only.
- Prometheus metrics endpoint: `/metrics` on a configurable port (default 9090). Tracks gRPC call outcomes and browser pool wait times.
- gRPC health check: `tonic-health` reporter so Kubernetes probes work without auth.
- `PooledBrowserFactory` with per-slot semaphores, DNS resolution for Chromium 131+ CDP Host-header rejection, and noVNC viewer URL per slot.
- Chromiumoxide vendored patch in `patches/` with protocol and SDK updates, wired via `.cargo/config.toml` patch replacement.
- Docker Compose CDP connectivity: `Dockerfile.chrome` with Xvfb + x11vnc + noVNC + socat for Chromium 131+ cross-container CDP access.
- `.env.template` with all available `IMAUTH_*` variables documented.

### Changed
- **Breaking (proto):** `LoginRequest` no longer accepts `username`/`password` fields. `Submit2Fa` and `SubmitCaptcha` RPCs removed from the service definition. `SessionState` simplified from `NeedsCreds`/`Needs2Fa`/`NeedsCaptcha` to a single `WaitingForUser` state. `AuthEvent` now includes `viewer_url` and `session_id` fields.
- **Breaking (CLI):** `login` command no longer takes `--username`/`--password`/`--2fa` flags. Instead, it prints the noVNC viewer URL and streams status until the user completes login in the browser.
- `PageDriver` trait simplified: removed `find_element`, `fill_input`, `click_element`, `click_element_text`, `press_enter`, `get_page_text` (automation methods). Added `close` for user-driven flow.
- gRPC methods now record metrics before error mapping so both success and failure paths are counted.
- SDKs updated for proto v0.3.0.0: Python and TypeScript clients handle the new `viewer_url` field and removed RPCs.
- Config default gRPC port changed from 50051 to 6100.
- Deleted `config/imauth.example.toml` (replaced by `.env.template`).

### Fixed
- Docker Compose CDP connectivity for Chromium 131+: Chrome binds CDP to 127.0.0.1 only, so a socat proxy forwards external 9222 to internal 9223.
- Python/TS SDK stale client references synced with proto v0.3.0.0 field names.

## [0.3.0.0] - 2026-05-16

### Added
- Python SDK API key support: pass `api_key=...` to `ImauthClient` / `AsyncImauthClient` to attach `authorization: Bearer <key>` to every call. Same parameter on TypeScript `new ImauthClient(addr, { apiKey })`. Without this, SDK clients couldn't reach an authenticated server.
- Python and TypeScript SDK wrappers for the previously missing RPCs: `submit_captcha`, `get_status`, `cancel`, `update_cookies`, `validate_session`, `delete_credentials`. SDKs can now drive every endpoint the server exposes.
- `AuthEvent.session_id` field in Python SDK and TS SDK, populated from the proto's `session_id`. Callers can now chain `Login` → `Submit2FA` / `Cancel` / `GetStatus` without losing the session token.
- `SessionBrowserRegistry` in `imauth-core`: binds `session_id` to the exact browser+page that ran `Login`, so `Submit2FA` types codes into the right tab even with concurrent logins. Previously the browser pool could return any browser, risking cross-session credential leaks.
- `ports/clock.rs` with `Clock` trait + `SystemClock` impl + `MockClock` test helper, restoring the port that was lost when NATS/refresh were removed in 0.2.0.0.
- `imauth-server::auth` module exposing `auth_interceptor` + `normalize_api_key` so the integration test exercises the same constant-time comparison and whitespace handling as the binary.
- `dotenvy::dotenv()` load on the imauth-server binary (matches the CLI). Operators get identical `.env` behavior on both binaries instead of silent asymmetry.
- Tests: 14 new SDK mock tests for `client.py`/`async_client.py`/`client.ts`; 13 gRPC converter tests in `grpc.rs`; 10 config tests; 8 server auth-interceptor tests; 4 snapshot-redaction tests; ActiveSessionRegistry tests covering register/take/discard/replace; plus regression tests for `proto_cookie_from`, browser-binding orphan handling, login send-failure logging, and session-create-failure event shape.

### Changed
- **Breaking (TypeScript SDK):** `submit2FA`, `getStatus`, `cancel`, and credential calls now send proto fields in snake_case (`session_id`, `twofa_method`) to match `proto-loader` `keepCase: true`. The previous camelCase keys were silently dropped on the wire, so any caller upgrading from 0.1.0 must regenerate or re-test against the server. Submit2FA was 100% broken before this fix.
- **Breaking (Python SDK):** `AuthEvent` adds a `session_id` field; helpers moved to `imauth._converters`. The leading-underscore aliases in `client.py` / `async_client.py` remain for backward-compat with mock-based tests.
- gRPC `Login` server now runs with `Server::serve_with_shutdown` so SIGTERM/Ctrl-C drains in-flight streams instead of leaking browser-pool permits.
- Login use case wires `tx.is_closed()` checks between every phase (browser acquire, page open, driver.login) and logs when an event drops because the client disconnected — previous code silently swallowed the failure and kept driving Chromium.
- `Session::new` when the DB write fails now emits a Final event with an empty `session_id` (instead of a fresh UUID that never persisted) so callers can distinguish "never created" from "created then failed".
- SQLite db dir and `.db`/`-wal`/`-shm` files now created with mode `0o700` / `0o600`. Defends encrypted credentials and cookies against other host UIDs when `/data` is bind-mounted.
- Pinned `selenium/standalone-chromium` Docker image to `131.0` so Chromium / CDP / login selectors don't shift under `:latest` upstream drift.
- Removed vestigial `imauth-core` dependency from `imauth-cli` (CLI is a pure tonic client; the dep was inherited from the pre-hexagonal layout).
- `ChromiumOxideBrowserSession::inner` now returns `Result<&Browser>` instead of panicking via `expect()`, surfacing pool-state bugs through the normal `ImauthError` path so a future code path can't accidentally panic inside a spawned task and orphan a permit.
- Cleaned up obsolete `#[allow(unused_imports)]` on `SqliteSessionRepository` and `SqliteRefreshTokenRepository`.

### Fixed
- Python SDK `client.py`/`async_client.py` referenced `Submit2FaRequest` (lowercase `a`) but the proto exports `Submit2FARequest` — every `submit_2fa()` call against a real server was `AttributeError`-ing. Fixed both clients to use the correct name.
- Python SDK `get_status()` used `isinstance(resp.status, str)` to decide between the enum lookup and an `IDLE` fallback; the proto returns an `int`, so the conditional was always false and `get_status` always returned `IDLE` regardless of the real session state. Replaced with the shared `_STATUS_MAP` int→`AuthStatus` translation in both sync and async clients.
- `UpdateCookiesRequest` proto skipped field tag 1 with no `reserved` clause — a future schema change could have silently mis-parsed against older clients. Added `reserved 1;`.
- Python SDK `get_credentials` already mapped `NOT_FOUND` to `None`; new `delete_credentials` and `cancel` follow the same idempotent pattern.

### Security
- Closed cross-session 2FA tab-poisoning risk in Submit2FA (see SessionBrowserRegistry above).
- Tightened SQLite database file permissions to 0600.
- Pinned Docker base image to avoid silent Chromium upgrades changing login selectors mid-deploy.

## [0.2.0.0] - 2026-05-15

### Added
- API key authentication for gRPC services via `--api-key` flag or `IMAUTH_API_KEY` env var on both server and CLI; requests authenticate with `Authorization: Bearer <key>` or `x-api-key` metadata
- Tests covering API key rejection for missing/wrong keys, acceptance for valid keys, and rejection of empty/whitespace keys
- `IMAUTH_API_KEY` wired into `docker-compose.yml` so the default deploy can require auth

### Changed
- **Breaking:** Encryption key is now required. Server refuses to start without `IMAUTH_ENCRYPTION_KEY` (or `[security].encryption_key`); the previous fallback to a process-lifetime random key has been removed
- gRPC errors no longer leak internal error detail to clients — internal errors are logged server-side and returned as generic "Internal server error"
- Chrome CDP CORS restricted from `*` to `http://localhost:50051`; the public CDP port (9222) is no longer exposed by docker-compose
- `/data` directory in container hardened with `chmod 700`
- API key comparison uses constant-time equality (`subtle::ConstantTimeEq`) to avoid timing side channels
- Empty or whitespace-only `IMAUTH_API_KEY` is now treated as unset rather than accepted as a valid bearer token

### Fixed
- Handle transient page context errors in Chrome driver and switch to CDP-native typing for more reliable input
- Login stream now emits a terminal `Failed` event when session creation fails, instead of closing silently with an empty stream
- `scripts/chrome-entrypoint.sh` TERM/INT trap previously passed quote-escaped strings to `kill`, leaving processes to be killed by Docker SIGKILL on shutdown

## [0.1.0.0] - 2026-05-12

### Added
- Extract shared protobuf stubs into new `imauth-proto` crate for reuse across server and CLI
- Server integration tests for cookie and credential CRUD operations
- TypeScript SDK jest test infrastructure and initial client tests
- Chrome entrypoint script for Docker container CDP proxy setup

### Changed
- Refactor core to hexagonal architecture with explicit domain, ports, adapters, and application layers
- Update `LoginUseCase` to support multiple platform drivers via `HashMap<Platform, Arc<dyn PlatformDriver>>`
- Return cookies directly from login and 2FA flows instead of storing in session state
- Centralize gRPC error mapping with `map_auth_err` helper
- Replace Docker `alpine-chrome` with `selenium/standalone-chromium` for better stability

### Removed
- NATS queue infrastructure (adapter, domain models, ports, and configuration)
- Per-crate protobuf generation (`build.rs` and `generated/mod.rs` in CLI and server)
- `refresh` application use case
- `natsonly.example.toml` configuration file
