# imauth

Browser-automation-based authentication service with gRPC API. Supports headless login flows and user-driven Kasm browser viewer for 2FA, CAPTCHA, and other interactive challenges.

## Quick Start (Docker Compose)

```bash
# 1. Copy the example env file and fill in secrets
cp .env.template .env
# Edit .env:
#   - IMAUTH_ENCRYPTION_KEY  (required, 32-byte base64)
#   - IMAUTH_VIEWER_TOKEN    (required, generate with `openssl rand -hex 32`)
#   - IMAUTH_API_KEY         (required, generate with `openssl rand -hex 32`)

# 2. Start all services
docker compose up -d

# 3. Verify
docker compose ps
```

### Default Exposed Ports

| Host Port | Container / Service | Purpose                      | Required             |
| --------- | ------------------- | ---------------------------- | -------------------- |
| **6100**  | `server:50051`      | gRPC API                     | ✅ Always            |
| **6101**  | `chrome-0-proxy:8080` | Kasm browser viewer (loopback by default) | ✅ User-driven login |

> **Note:** All external ports live in the `610X` range to avoid collisions with other services.

The viewer proxy requires `IMAUTH_VIEWER_TOKEN`. A valid token in the generated
viewer URL is exchanged for an `HttpOnly`, `SameSite=Strict` session cookie and
immediately redirected to the same path without the token. The cookie authorizes
subsequent assets and WebSocket connections. Proxy access logs are disabled,
and viewer responses disable caching and referrers and apply restrictive CSP,
frame, content-type, and permissions headers.

The gRPC API and viewer are both published only on `127.0.0.1`, and
Compose refuses to start when the API key, viewer token, or encryption key is
missing. Compose does not provide a non-loopback bind override because its
published services do not terminate TLS. For remote viewer access, publish the
loopback endpoint through Tailscale or an HTTPS reverse proxy, then set
`IMAUTH_VIEWER_SCHEME=https` and `IMAUTH_VIEWER_COOKIE_SECURE=Secure`. Treat the
initial viewer URL as a secret until its first redirect.

### Internal-only Ports (not exposed to host)

| Port | Service        | Purpose                        |
| ---- | -------------- | ------------------------------ |
| 9222 | `chrome-0`     | Chrome DevTools Protocol (CDP) |

## Environment Variables

All environment variables use the `IMAUTH_` prefix.

| Variable                       | Default     | Description                                      |
| ------------------------------ | ----------- | ------------------------------------------------ |
| `IMAUTH_ENCRYPTION_KEY`        | —           | **Required.** 32-byte base64-encoded AES-256 key |
| `IMAUTH_API_KEY`               | —           | **Required.** gRPC API key for client auth       |
| `IMAUTH_VIEWER_TOKEN`          | —           | **Required.** Static noVNC viewer access token   |
| `IMAUTH_HOSTNAME`              | `localhost` | Hostname used in Kasm browser viewer URLs        |
| `IMAUTH_HOSTPORT`              | `6100`      | Host port mapped to gRPC (50051)                 |
| `IMAUTH_NOVNC_PORT_0`          | `6101`      | Host port for noVNC viewer                        |
| `IMAUTH_VIEWER_SCHEME`         | `http`      | Public viewer URL scheme (`https` behind TLS)     |
| `IMAUTH_VIEWER_COOKIE_SECURE`  | —           | Set to `Secure` behind an HTTPS viewer endpoint  |
| `IMAUTH_DATA`                  | `../imauth-data` | Host directory for persisted chrome/server data |


## Running Multiple Instances on One Host

Assign ports in non-overlapping groups:

| Instance   | gRPC | viewer port |
| ---------- | ---- | ----------- |
| Instance A | 6100 | 6101        |
| Instance B | 6110 | 6111        |
| Instance C | 6120 | 6121        |

Update `.env` accordingly before starting each stack.

## Container Images

Release images are published to `docker.lowapple.io` on every `v*` tag:

| Image                       | Contents                               |
| --------------------------- | -------------------------------------- |
| `docker.lowapple.io/imauth`              | gRPC server + CLI (slim runtime)       |
| `docker.lowapple.io/imauth-chrome`       | Chromium + Kasm browser viewer sidecar |
| `docker.lowapple.io/imauth-chrome-proxy` | Authenticated browser viewer proxy     |

```bash
# Pull a specific release (or :latest)
docker pull docker.lowapple.io/imauth:v0.7.0
docker pull docker.lowapple.io/imauth-chrome:v0.7.0
docker pull docker.lowapple.io/imauth-chrome-proxy:v0.7.0
```

Local image builds use the Makefile:

```bash
make docker              # build docker.lowapple.io/imauth
make docker-chrome       # build docker.lowapple.io/imauth-chrome
make docker-chrome-proxy # build docker.lowapple.io/imauth-chrome-proxy
make deploy-all     # buildx multi-arch build + push all images
```

## SDKs

Source lives in `sdk/python/` and `sdk/typescript/`.

### Python — install from a GitHub Release

The wheel is built (with proto stubs regenerated) and attached to each GitHub
Release, so install it directly without cloning or running `protoc`:

```bash
# Replace the tag/version with the one from the Releases page
pip install https://github.com/imyounjs/imauth/releases/download/v0.7.0/imauth-0.7.0-py3-none-any.whl
```

```python
from imauth import ImauthClient
from imauth.models import Platform

client = ImauthClient("localhost:6100", api_key="<IMAUTH_API_KEY>")
for event in client.login(Platform.INSTAGRAM):
    print(event.status, event.viewer_url)  # open viewer_url to finish login
```

> Installing from source (`pip install "git+https://github.com/imyounjs/imauth.git#subdirectory=sdk/python"`)
> is **not** supported: the generated `imauth/v1/*_pb2.py` stubs are gitignored
> and only produced during the release build. Use the published wheel instead.

#### CLI via `uvx`

The wheel ships an `imauth` console script, so you can drive the server from a
shell without installing anything permanently:

```bash
# Point uvx at the release wheel; everything after `imauth` is the CLI
WHL=https://github.com/imyounjs/imauth/releases/download/v0.7.0/imauth-0.7.0-py3-none-any.whl

export IMAUTH_URL=localhost:6100
export IMAUTH_API_KEY=<key>

uvx --from "$WHL" imauth login --platform naver        # stream login events (JSON)
uvx --from "$WHL" imauth validate --platform naver     # exit 0 if session present
uvx --from "$WHL" imauth connections                   # status of all platforms
uvx --from "$WHL" imauth --help                         # full command list
```

`--server` / `--api-key` flags override the `IMAUTH_URL` /
`IMAUTH_API_KEY` env vars. For `creds-save`, prefer `IMAUTH_CRED_PASSWORD` over
`--password` so the secret never lands in your shell history.

### TypeScript

```bash
npm install   # from sdk/typescript/
```

## Provider onboarding recorder

Record a real login surface before adding a provider that imauth does not yet
support. The default opens an isolated, headed Chromium browser:

```bash
./scripts/provider-record \
  --url https://nid.naver.com/nidlogin.login \
  --domain naver.com
```

The installed Rust CLI exposes the same command and embeds the recorder, so a
source checkout is not required:

```bash
imauth provider record --url <https-login-url> --domain <provider-domain>
```

Node.js and npm are required. On first use, the CLI creates a versioned user
cache with Playwright 1.62.1 and Chromium. This cache is outside the repository.
Use `checkpoint <name>` while exercising login states, then `finish`; `abort`
keeps an incomplete capture. Pass `--cdp-url http://127.0.0.1:9222` to observe
an existing imauth/Chrome session. `--headless --auto-finish` is available for
non-interactive smoke checks.

Records are written to `datasource/records/<domain>-<timestamp>/`. The standard
record is deliberately small and sanitized-only: HTML checkpoints,
cookie/storage state, HAR, console warnings/errors, and a first-party JavaScript
URL/hash inventory. Automatic smoke checks keep one final checkpoint.

Use `--deep` only when that evidence cannot explain an authentication bug. Deep
mode additionally records raw values, screenshots, JavaScript bodies, and a
Playwright trace under the gitignored `raw/` directory. Only consider sanitized
artifacts, the manifest, report, and redaction report for Git, and only when
`readyForGit` is true after manual review. Enter credentials and 2FA values only
in the browser.

## Development

See [`AGENTS.md`](AGENTS.md) for build commands, testing guidelines, and project conventions.

Run `make install-hooks` once after cloning. The pre-commit hook runs `make quality`,
which checks Rust, Python SDK, and TypeScript SDK linting and formatting.
