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

| Host Port | Container / Service | Purpose                      | Required             |
| --------- | ------------------- | ---------------------------- | -------------------- |
| **6100**  | `chrome-0:50051`    | gRPC API                     | ✅ Always            |
| **6101**  | `chrome-0:6080`     | Kasm browser viewer (slot 0) | ✅ User-driven login |
| **6102**  | `chrome-1:6080`     | Kasm browser viewer (slot 1) | ✅ User-driven login |
| **6103**  | `chrome-2:6080`     | Kasm browser viewer (slot 2) | ✅ User-driven login |
| 9090      | `server:9090`       | Prometheus metrics           | Optional             |

> **Note:** All external ports live in the `610X` range to avoid collisions with other services.

### Internal-only Ports (not exposed to host)

| Port | Service        | Purpose                        |
| ---- | -------------- | ------------------------------ |
| 9222 | `chrome-0/1/2` | Chrome DevTools Protocol (CDP) |

## Environment Variables

All environment variables use the `IMAUTH_` prefix.

| Variable                       | Default     | Description                                      |
| ------------------------------ | ----------- | ------------------------------------------------ |
| `IMAUTH_ENCRYPTION_KEY`        | —           | **Required.** 32-byte base64-encoded AES-256 key |
| `IMAUTH_API_KEY`               | —           | gRPC API key for client auth                     |
| `IMAUTH_SERVER_HOSTNAME`       | `localhost` | Hostname used in Kasm browser viewer URLs        |
| `IMAUTH_GRPC_HOST_PORT`        | `6100`      | Host port mapped to gRPC (50051)                 |
| `IMAUTH_BROWSER_VIEWER_PORT_0` | `6101`      | Host port for browser viewer slot 0              |
| `IMAUTH_BROWSER_VIEWER_PORT_1` | `6102`      | Host port for browser viewer slot 1              |
| `IMAUTH_BROWSER_VIEWER_PORT_2` | `6103`      | Host port for browser viewer slot 2              |
| `IMAUTH_METRICS_PORT`          | `9090`      | Host port for Prometheus metrics                 |

## Running Multiple Instances on One Host

Assign ports in non-overlapping groups:

| Instance   | gRPC | viewer ports |
| ---------- | ---- | ------------ |
| Instance A | 6100 | 6101–6103    |
| Instance B | 6110 | 6111–6113    |
| Instance C | 6120 | 6121–6123    |

Update `.env` accordingly before starting each stack.

## Container Images

Release images are published to GitHub Container Registry (GHCR) on every `v*` tag:

| Image                            | Contents                               |
| -------------------------------- | -------------------------------------- |
| `ghcr.io/soapbird/imauth`        | gRPC server + CLI (slim runtime)       |
| `ghcr.io/soapbird/imauth-chrome` | Chromium + Kasm browser viewer sidecar |

```bash
# Pull a specific release (or :latest)
docker pull ghcr.io/soapbird/imauth:v0.4.1
docker pull ghcr.io/soapbird/imauth-chrome:v0.4.1
```

> GHCR images are private by default — authenticate first with
> `echo $GITHUB_TOKEN | docker login ghcr.io -u <username> --password-stdin`
> (token needs `read:packages`).

Local image builds use the Makefile:

```bash
make docker         # build ghcr.io/soapbird/imauth
make docker-chrome  # build ghcr.io/soapbird/imauth-chrome
make deploy-all     # buildx multi-arch build + push both images
```

## SDKs

Source lives in `sdk/python/` and `sdk/typescript/`.

### Python — install from a GitHub Release

The wheel is built (with proto stubs regenerated) and attached to each GitHub
Release, so install it directly without cloning or running `protoc`:

```bash
# Replace the tag/version with the one from the Releases page
pip install https://github.com/soapbird/imauth/releases/download/v0.4.1/imauth-0.4.1-py3-none-any.whl
```

```python
from imauth import ImauthClient
from imauth.models import Platform

client = ImauthClient("localhost:6100", api_key="<IMAUTH_API_KEY>")
for event in client.login(Platform.INSTAGRAM):
    print(event.status, event.viewer_url)  # open viewer_url to finish login
```

> Installing from source (`pip install "git+https://github.com/soapbird/imauth.git#subdirectory=sdk/python"`)
> is **not** supported: the generated `imauth/v1/*_pb2.py` stubs are gitignored
> and only produced during the release build. Use the published wheel instead.

#### CLI via `uvx`

The wheel ships an `imauth` console script, so you can drive the server from a
shell without installing anything permanently:

```bash
# Point uvx at the release wheel; everything after `imauth` is the CLI
WHL=https://github.com/soapbird/imauth/releases/download/v0.4.1/imauth-0.4.1-py3-none-any.whl

export IMAUTH_SERVER_ADDRESS=localhost:6100
export IMAUTH_API_KEY=<key>

uvx --from "$WHL" imauth login --platform naver        # stream login events (JSON)
uvx --from "$WHL" imauth validate --platform naver     # exit 0 if session present
uvx --from "$WHL" imauth connections                   # status of all platforms
uvx --from "$WHL" imauth --help                         # full command list
```

`--server` / `--api-key` flags override the `IMAUTH_SERVER_ADDRESS` /
`IMAUTH_API_KEY` env vars. For `creds-save`, prefer `IMAUTH_CRED_PASSWORD` over
`--password` so the secret never lands in your shell history.

### TypeScript

```bash
npm install   # from sdk/typescript/
```

## Development

See [`AGENTS.md`](AGENTS.md) for build commands, testing guidelines, and project conventions.
