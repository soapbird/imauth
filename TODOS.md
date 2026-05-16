# TODOS

Tracked follow-up work, grouped by component then priority.

## CI/CD

### CI smoke for docker-compose
**Priority:** P2
**Origin:** /ship v0.3.0.0 — Red Team finding (image pin)
**Why:** Pinned `selenium/standalone-chromium:131.0` reduces drift but a CI job that brings the stack up and hits `localhost:9222/json/version` would catch base-image regressions before a developer is bitten.

## SDK

### Python and TypeScript SDK contract tests against a real server
**Priority:** P1
**Origin:** /ship v0.3.0.0 — testing specialist (E2E gaps)
**Why:** Current SDK coverage is ~85% via mocks, but no test starts the real Rust server and drives every public method end-to-end. The TS camelCase / proto-loader keepCase mismatch (Submit2FA broken) would have been caught by even one E2E test.
**Acceptance:** `make sdk-e2e` (or equivalent) starts `imauth-server` with a test API key, walks each SDK method, asserts wire-level proto field names against the response. CI runs the test on every push.

### Python / TS SDK error contract
**Priority:** P2
**Origin:** /ship v0.3.0.0 — security + api-contract specialists
**Why:** SDKs currently leak raw `grpc.RpcError` / `AioRpcError` / Node `ServiceError` on most failure paths. A typed `ImauthAuthError` / `ImauthConnectionError` / `ImauthNotFoundError` mapping with one central wrapper would let downstream apps catch domain errors without grpc internals.

## Server

### gRPC Login stream cancellation via CancellationToken
**Priority:** P1
**Origin:** /ship v0.3.0.0 — Red Team (login work leaks browser permit on client disconnect)
**Why:** Login adds `tx.is_closed()` checks between phases (good), but the spawned task can still be mid-driver.login() inside chromiumoxide when the client drops; only the next phase boundary catches the cancellation. A `tokio_util::sync::CancellationToken` racing with the driver call would abort immediately and free the browser permit.
**Acceptance:** Integration test starts a Login stream, drops the client after `Started`, asserts the browser pool count returns to baseline within N seconds and no orphaned `tracing::warn!` lines about send failures linger.

### Drop unused `_test_stubs` / private aliases once SDK mock tests are reworked
**Priority:** P3
**Origin:** /ship v0.3.0.0 — testing specialist
**Why:** `imauth.client._auth_event_from_proto` aliases exist purely so the existing mock test file keeps working. Future tests should import directly from `imauth._converters`; once the mock suite is refactored to use stub-injection instead of `patch.object(client_module, ...)`, the aliases can go.

### Adapter: cookie bulk insert via QueryBuilder
**Priority:** P2
**Origin:** /ship v0.3.0.0 — performance specialist
**Why:** Cookie save loops one prepare/bind/execute per row (N+1 for bulk inserts). After a successful login the cookie set is 20+ rows. A single multi-VALUES INSERT + ON CONFLICT DO UPDATE would cut DB round-trips proportionally.
**Acceptance:** `SqliteCookieRepository::save` uses `sqlx::QueryBuilder::push_values`; perf test confirms one query per save regardless of cookie count.

### `GetConnectionStatusUseCase` single-query lookup
**Priority:** P3
**Origin:** /ship v0.3.0.0 — performance specialist
**Why:** Currently fans out one `cookies.get()` per platform via `join_all`. SQLite serializes through the pool anyway, so a single `SELECT DISTINCT platform FROM cookies WHERE name IN (...)` is strictly better. Low priority because Platform::ALL is only 2 today.

### `redact_html_snapshot` performance + screenshot redaction
**Priority:** P2
**Origin:** /ship v0.3.0.0 — performance + security specialists
**Why:** Allocates a full lowercase copy of the entire HTML body on every snapshot. Runs synchronously inside an async fn (blocks the executor for multi-MB pages). Also, PNG screenshots are still saved unredacted next to the redacted HTML — visible 2FA codes leak via the screenshot.
**Acceptance:** scan with `memchr` + `eq_ignore_ascii_case`; gate PNG capture on a debug flag (off in production); document `/data` perms in the deploy guide.

### Constant-time API-key length leak
**Priority:** P3
**Origin:** /ship v0.3.0.0 — security specialist
**Why:** `subtle::ConstantTimeEq::ct_eq` short-circuits on length mismatch, leaking the key's byte length via response timing to a remote attacker.
**Acceptance:** Hash both keys with SHA-256 first then `ct_eq` the digests; assert variance below threshold in a regression test.

### CDP proxy bind to 127.0.0.1
**Priority:** P2
**Origin:** /ship v0.3.0.0 — security + red-team specialists
**Why:** Python TCP proxy in `scripts/chrome-entrypoint.sh` binds CDP on 0.0.0.0:9222 inside the shared netns. Server shares the netns so 127.0.0.1 works just as well and eliminates the surface for any future sidecar.

### Better fill_input error messaging
**Priority:** P4
**Origin:** /ship v0.3.0.0 — maintainability specialist
**Why:** After clear-step failure, `fill_input` returns the same "Could not focus input X" error as the initial focus failure. Operators chase the wrong cause.

## Completed

### Add GitHub Actions release pipeline
**Completed:** v0.3.0.0 (2026-05-16)
**Origin:** /ship v0.3.0.0 — D1
Shipped as `.github/workflows/release.yml` (multi-arch Rust build matrix on tag `v*` push) alongside `.github/workflows/test.yml` for PR CI.
