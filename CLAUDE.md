# CLAUDE.md

## Project Overview

`imauth` is a Rust gRPC auth service for browser-driven platform login (Instagram, etc.), with Python and TypeScript SDKs. Current version: see `VERSION`.

## Architecture (hexagonal)

`crates/imauth-core` is split by hexagonal layer:
- `domain/` — pure types (`Session`, `Platform`, errors). No I/O.
- `ports/` — trait boundaries: `browser` (pool + driver), `repository` (sessions, cookies, credentials), `encryption`, `clock` (`Clock` trait + `SystemClock` + `MockClock` test helper, restored in 0.3.0.0).
- `adapters/` — concrete implementations: `chromiumoxide` (CDP browser pool + per-platform `PageDriver` impls), `sqlite` (file-mode `0o600` repos), `aes_gcm` (encryption), `fs` (snapshot with HTML redaction), `inmem` (test doubles).
- `application/` — use cases: `login` (streams `AuthEvent`s, gated by `tx.is_closed()` checks between phases), `submit_2fa`, `status`, `cookies`, `credentials`, `container` (dependency wiring), `active_session` (the `SessionBrowserRegistry` that pins a `session_id` to the exact browser+page that ran `Login`, so `Submit2FA` types codes into the right tab even under concurrent logins).

`crates/imauth-server` is a thin gRPC adapter over `imauth-core`:
- `src/auth.rs` — extracted `auth_interceptor` + `normalize_api_key` (constant-time bearer check, whitespace-trim). Server binary and integration tests share this exact code path.
- `src/grpc.rs` — proto ↔ domain mapping. All errors funnel through `map_auth_err` so internal detail never leaks to clients.
- `src/main.rs` — `dotenvy::dotenv()` first, then `Server::serve_with_shutdown` so SIGTERM/Ctrl-C drains in-flight `Login` streams instead of leaking browser-pool permits.

`crates/imauth-cli` is a pure `tonic` client (no `imauth-core` dep). `crates/imauth-proto` owns generated stubs.

## SDK clients

`sdk/python` and `sdk/typescript` both accept an `api_key` / `apiKey` option and attach `authorization: Bearer <key>` to every call. Both expose the full RPC surface: `login`, `submit_2fa`, `submit_captcha`, `get_status`, `cancel`, `update_cookies`, `validate_session`, plus credential and cookie CRUD. Wire format is snake_case (`proto-loader keepCase: true` on the TS side) — camelCase keys are silently dropped, so always use snake_case in fixtures.

`AuthEvent.session_id` is populated from the proto field of the same name. Chain calls via `Login → Submit2FA / Cancel / GetStatus` by passing that id back in.

## Required env vars

- `IMAUTH_ENCRYPTION_KEY` — required, server refuses to start without it.
- `IMAUTH_API_KEY` — required when auth is enabled (default in `docker-compose.yml`).
- Both binaries load `.env` via `dotenvy` at startup.

## GBrain Configuration (configured by /setup-gbrain)
- Mode: local-stdio
- Engine: postgres
- Config file: ~/.gbrain/config.json (mode 0600)
- Setup date: 2026-05-11
- MCP registered: yes
- Artifacts sync: full
- Current repo policy: read-write

## GBrain Search Guidance (configured by /sync-gbrain)
<!-- gstack-gbrain-search-guidance:start -->

GBrain is set up and synced on this machine. The agent should prefer gbrain
over Grep when the question is semantic or when you don't know the exact
identifier yet. Two indexed corpora available via the `gbrain` CLI:
- This repo's code (registered as `gstack-code-<repo>` source).
- `~/.gstack/` curated memory (registered as `gstack-brain-<user>` source via
  the existing federation pipeline).

Prefer gbrain when:
- "Where is X handled?" / semantic intent, no exact string yet:
    `gbrain search "<terms>"` or `gbrain query "<question>"`
- "Where is symbol Y defined?" / symbol-based code questions:
    `gbrain code-def <symbol>` or `gbrain code-refs <symbol>`
- "What calls Y?" / "What does Y depend on?":
    `gbrain code-callers <symbol>` / `gbrain code-callees <symbol>`
- "What did we decide last time?" / past plans, retros, learnings:
    `gbrain search "<terms>" --source gstack-brain-<user>`

Grep is still right for known exact strings, regex, multiline patterns, and
file globs. The brain auto-syncs incrementally on every gstack skill start.
Run `/sync-gbrain` to force-refresh, `/sync-gbrain --full` for full reindex.

<!-- gstack-gbrain-search-guidance:end -->
