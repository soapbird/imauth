# Changelog

All notable changes to this project will be documented in this file.

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
