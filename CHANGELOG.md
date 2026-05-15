# Changelog

All notable changes to this project will be documented in this file.

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
