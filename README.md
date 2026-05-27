# imauth

Browser-automation-based authentication service with gRPC API. Supports headless login flows and user-driven Kasm browser viewer for 2FA, CAPTCHA, and other interactive challenges.

## Quick Start (Docker Compose)

```bash
# 1. Copy the example env file and fill in secrets
cp .env.template .env
# Edit .env:
#   - IMAUTH_ENCRYPTION_KEY  (required, 32-byte base64)
#   - IMAUTH_API_KEY         (recommended for production)

# 2. Start all services
docker compose up -d

# 3. Verify
docker compose ps
```

### Default Exposed Ports

| Host Port | Container / Service | Purpose | Required |
|-----------|---------------------|---------|----------|
| **6100** | `chrome-0:50051` | gRPC API | ✅ Always |
| **6101** | `chrome-0:6080` | Kasm browser viewer (slot 0) | ✅ User-driven login |
| **6102** | `chrome-1:6080` | Kasm browser viewer (slot 1) | ✅ User-driven login |
| **6103** | `chrome-2:6080` | Kasm browser viewer (slot 2) | ✅ User-driven login |
| 9090 | `server:9090` | Prometheus metrics | Optional |

> **Note:** All external ports live in the `610X` range to avoid collisions with other services.

### Internal-only Ports (not exposed to host)

| Port | Service | Purpose |
|------|---------|---------|
| 9222 | `chrome-0/1/2` | Chrome DevTools Protocol (CDP) |
| 5432 | `imauth-postgres` | PostgreSQL database |

## Environment Variables

All environment variables use the `IMAUTH_` prefix.

| Variable | Default | Description |
|----------|---------|-------------|
| `IMAUTH_ENCRYPTION_KEY` | — | **Required.** 32-byte base64-encoded AES-256 key |
| `IMAUTH_API_KEY` | — | gRPC API key for client auth |
| `IMAUTH_SERVER_HOSTNAME` | `localhost` | Hostname used in Kasm browser viewer URLs |
| `IMAUTH_GRPC_HOST_PORT` | `6100` | Host port mapped to gRPC (50051) |
| `IMAUTH_BROWSER_VIEWER_PORT_0` | `6101` | Host port for browser viewer slot 0 |
| `IMAUTH_BROWSER_VIEWER_PORT_1` | `6102` | Host port for browser viewer slot 1 |
| `IMAUTH_BROWSER_VIEWER_PORT_2` | `6103` | Host port for browser viewer slot 2 |
| `IMAUTH_METRICS_PORT` | `9090` | Host port for Prometheus metrics |

## Running Multiple Instances on One Host

Assign ports in non-overlapping groups:

| Instance | gRPC | viewer ports |
|----------|------|-------------|
| Instance A | 6100 | 6101–6103 |
| Instance B | 6110 | 6111–6113 |
| Instance C | 6120 | 6121–6123 |

Update `.env` accordingly before starting each stack.

## SDKs

- **Python**: `sdk/python/`
- **TypeScript**: `sdk/typescript/`

## Development

See [`AGENTS.md`](AGENTS.md) for build commands, testing guidelines, and project conventions.
