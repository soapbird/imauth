# Repository Guidelines

## Project Overview

`imauth` orchestrates user-mediated Instagram, Threads, and Naver browser logins. It exposes gRPC APIs for login, session cookies, and credentials; drives Chrome through CDP; stores SQLite data encrypted with AES-256-GCM; and ships Rust server/CLI plus Python and TypeScript SDKs.

## Architecture & Data Flow

- Rust uses clean/hexagonal layers in `crates/imauth-core`: **adapters → ports → application → domain**. Keep `domain/` pure: no I/O, async, or `Arc`.
- Add behavior through the established path: domain type/state → async port trait → adapter → `XxxUseCase` → `AppContainer` wiring → server gRPC mapping → proto/SDK updates.
- `Login` flows from `imauth-server/src/grpc.rs` to `LoginUseCase`: create session, acquire a pooled CDP browser slot, stream a viewer URL, poll cookies, persist filtered/encrypted cookies, then emit the terminal event.
- gRPC boundaries own auth and error redaction. `imauth-server/src/auth.rs` accepts Bearer or `x-api-key`; `grpc.rs` translates domain errors to safe statuses. Do not leak storage, browser, or crypto errors.
- Proto is the cross-language contract in `proto/imauth/v1/`. Edit proto first; reserve removed field tags/enums rather than reusing them.

## Key Directories

- `crates/imauth-core/` — domain, application use cases, port traits, SQLite/CDP/AES adapters, config.
- `crates/imauth-server/` — `imauth-server serve`, tonic services, API-key interceptor, health service.
- `crates/imauth-cli/` — gRPC CLI; `provider record` is local Node/Playwright tooling.
- `crates/imauth-proto/` — `tonic_build` bindings. Never hand-edit `src/generated/imauth.v1.rs`.
- `proto/imauth/v1/` — source `.proto` files and service definitions.
- `sdk/python/` — Python 3.10+ synchronous/async clients, Pydantic models, pytest/Ruff.
- `sdk/typescript/` — TypeScript gRPC client, Jest, Biome, strict `tsc`.
- `scripts/` — local Chrome/server bootstrap and provider-record/redaction tooling.
- `patches/` — vendored Chromiumoxide patches; read `patches/PATCHES.md` before touching them.

## Development Commands

Run Cargo commands from the repository root: `.cargo/config.toml` supplies the required local `chromiumoxide` patch.

```bash
make build                 # release server and CLI
make check                 # cargo check --workspace
make test                  # Rust unit and integration tests
make proto                 # regenerate Rust proto bindings
make quality               # format-check + lint across Rust and SDKs
make format-check          # non-mutating formatting checks
make run-server            # local release server
make start-local           # local Chrome (CDP :9222) and server
buf lint
buf format --diff --exit-code
```

```bash
make -C sdk/python generate lint format-check test
npm --prefix sdk/typescript test
npm --prefix sdk/typescript run lint
npm --prefix sdk/typescript run format:check
npm --prefix sdk/typescript run build
```

Use `make up` / `make down` for published Docker images. Use `docker compose -f docker-compose.dev.yml up` for local image builds.

## Code Conventions & Common Patterns

- Rust: `rustfmt`, `clippy -D warnings`, `thiserror` via the shared `ImauthError`/`Result<T>`, and structured `tracing` fields.
- Ports are async traits and normally carry `#[cfg_attr(test, mockall::automock)]`; use cases expose `new()` and `async execute()`.
- Keep platform-specific login URLs, cookie domains, and session-cookie checks centralized in `crates/imauth-core/src/domain/platform.rs`.
- Treat session transitions in `domain/session.rs` as a state machine; do not bypass transition invariants.
- Unit tests are colocated in `#[cfg(test)] mod tests`; use behavior-oriented names and Given/When/Then comments where present. Serialize tests that mutate process environment variables.
- Python: Ruff, 88-column formatting, typed Pydantic-facing models; use `anyio` for async tests.
- TypeScript: Biome and strict TypeScript; prefer readonly types and validate gRPC wire values in `src/grpc_wire.ts`.
- Avoid compatibility shims. Update direct callers and all SDK contract tests with an intentional API change.

## Important Files

- `Cargo.toml`, `.cargo/config.toml` — workspace dependencies and Chromiumoxide overrides.
- `Makefile` — canonical local build, quality, and Docker commands.
- `crates/imauth-core/src/application/container.rs` — dependency composition root.
- `crates/imauth-core/src/config.rs` — TOML/environment configuration and defaults.
- `crates/imauth-server/src/main.rs`, `grpc.rs`, `auth.rs` — serving, API mapping, authentication.
- `crates/imauth-cli/src/cli_support.rs`, `provider_record.rs` — CLI implementation and embedded recorder.
- `docker-compose.yml`, `docker-compose.dev.yml`, `.env.template` — runtime topology and local configuration.
- `buf.yaml` — proto lint/breaking policy.

## Runtime/Tooling Preferences

- Rust workspace: edition 2021; container builds use Rust 1.88. Local Rust is not pinned.
- Docker topology is server + Chromium CDP + authenticated noVNC proxy. Default host gRPC is `localhost:6100`; the viewer is `localhost:6101`.
- Copy `.env.template` to `.env`; never commit encryption keys, API keys, viewer tokens, cookies, credentials, browser captures, or logs containing them.
- Node.js and npm are required for TypeScript and `provider record`; Python SDK generation requires `grpc_tools.protoc`.
- Do not remove or relocate `.cargo/config.toml`, and do not edit `patches/` outside the documented dedicated patch workflow.
- `provider record` captures sensitive browser data. Commit only reviewed `sanitized/`, `manifest.json`, `redaction-report.json`, and `report.md` when `readyForGit` is true; never commit `raw/` artifacts.

## Testing & QA

- Rust: `make test`; focus server wire coverage with `cargo test -p imauth-server --test integration_test`.
- Rust integration tests use real tonic services and in-memory SQLite with fake browser ports; do not require a live browser or server.
- Python tests are pytest stub-based. Run `make -C sdk/python generate` before tests on a fresh checkout because generated bindings are ignored.
- TypeScript tests use Jest/ts-jest; `npm test` copies proto sources automatically.
- For proto changes: run Buf checks, regenerate Rust/Python bindings, run TypeScript tests/build, and update enum/field contract tests in every SDK.
- For recorder/redaction changes: run `node --test scripts/`.
