#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
empty_env=$(mktemp)
compose_output=$(mktemp)
headers=$(mktemp)
cookie_jar=$(mktemp)
body=$(mktemp)
suffix=$$
network="imauth-viewer-security-$suffix"
backend="imauth-viewer-backend-$suffix"
proxy="imauth-viewer-proxy-$suffix"
secure_proxy="imauth-viewer-secure-proxy-$suffix"
cleanup() {
  docker rm -f "$secure_proxy" "$proxy" "$backend" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  rm -f "$empty_env" "$compose_output" "$headers" "$cookie_jar" "$body"
}
trap cleanup EXIT HUP INT TERM

verify_compose_contract() {
  compose_file=$1

  if env -u IMAUTH_API_KEY \
    IMAUTH_VIEWER_TOKEN=test-viewer-token-00000000000000000000 \
    IMAUTH_ENCRYPTION_KEY=test-encryption-key \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output" 2>&1; then
    echo "$compose_file unexpectedly accepted a missing API key" >&2
    exit 1
  fi

  if env -u IMAUTH_VIEWER_TOKEN \
    IMAUTH_API_KEY=test-api-key-000000000000000000000000 \
    IMAUTH_ENCRYPTION_KEY=test-encryption-key \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output" 2>&1; then
    echo "$compose_file unexpectedly accepted a missing viewer token" >&2
    exit 1
  fi

  if env -u IMAUTH_ENCRYPTION_KEY \
    IMAUTH_API_KEY=test-api-key-000000000000000000000000 \
    IMAUTH_VIEWER_TOKEN=test-viewer-token-00000000000000000000 \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output" 2>&1; then
    echo "$compose_file unexpectedly accepted a missing encryption key" >&2
    exit 1
  fi

  if IMAUTH_API_KEY= \
    IMAUTH_VIEWER_TOKEN=test-viewer-token-00000000000000000000 \
    IMAUTH_ENCRYPTION_KEY=test-encryption-key \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output" 2>&1; then
    echo "$compose_file unexpectedly accepted an empty API key" >&2
    exit 1
  fi

  if IMAUTH_VIEWER_TOKEN= \
    IMAUTH_API_KEY=test-api-key-000000000000000000000000 \
    IMAUTH_ENCRYPTION_KEY=test-encryption-key \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output" 2>&1; then
    echo "$compose_file unexpectedly accepted an empty viewer token" >&2
    exit 1
  fi

  if IMAUTH_ENCRYPTION_KEY= \
    IMAUTH_API_KEY=test-api-key-000000000000000000000000 \
    IMAUTH_VIEWER_TOKEN=test-viewer-token-00000000000000000000 \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output" 2>&1; then
    echo "$compose_file unexpectedly accepted an empty encryption key" >&2
    exit 1
  fi

  IMAUTH_API_KEY=test-api-key-000000000000000000000000 \
    IMAUTH_VIEWER_TOKEN=test-viewer-token-00000000000000000000 \
    IMAUTH_ENCRYPTION_KEY=test-encryption-key \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output"

  [ "$(grep -Fc 'host_ip: 127.0.0.1' "$compose_output")" -eq 2 ]
  grep -Fq 'published: "6100"' "$compose_output"
  grep -Fq 'published: "6101"' "$compose_output"

  IMAUTH_API_KEY=test-api-key-000000000000000000000000 \
  IMAUTH_VIEWER_TOKEN=test-viewer-token-00000000000000000000 \
  IMAUTH_ENCRYPTION_KEY=test-encryption-key \
  IMAUTH_BIND_HOST=0.0.0.0 \
    docker compose --env-file "$empty_env" -f "$compose_file" config >"$compose_output"
  [ "$(grep -Fc 'host_ip: 127.0.0.1' "$compose_output")" -eq 2 ]
  if grep -Fq 'host_ip: 0.0.0.0' "$compose_output"; then
    echo "$compose_file accepted a non-loopback bind override" >&2
    exit 1
  fi
}

verify_compose_contract "$repo_root/docker-compose.yml"
verify_compose_contract "$repo_root/docker-compose.dev.yml"

proxy_script="$repo_root/scripts/chrome-proxy-entrypoint.sh"
token=test-viewer-token-00000000000000000000

docker network create "$network" >/dev/null
docker run --rm -d --name "$backend" --network "$network" nginx:alpine >/dev/null
docker run --rm -d --name "$proxy" --network "$network" \
  -p 127.0.0.1:0:8080 \
  -e "UPSTREAM_HOST=$backend" \
  -e UPSTREAM_PORT=80 \
  -e "IMAUTH_VIEWER_TOKEN=$token" \
  -v "$proxy_script:/chrome-proxy-entrypoint.sh:ro" \
  --entrypoint /bin/sh \
  nginx:alpine /chrome-proxy-entrypoint.sh >/dev/null

port=$(docker port "$proxy" 8080/tcp | sed -n 's/.*://p')
attempt=0
until curl -sS -o /dev/null "http://127.0.0.1:$port/"; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 20 ]; then
    echo "viewer proxy did not become ready" >&2
    exit 1
  fi
  sleep 1
done

status=$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/")
[ "$status" = 403 ]

curl -sS -D "$headers" -o /dev/null -c "$cookie_jar" \
  "http://127.0.0.1:$port/index.html?token=$token"
grep -Eq '^HTTP/[^ ]+ 303' "$headers"
grep -Eiq '^Location: /index\.html\?enable_webp=0\r?$' "$headers"
if grep -Eiq '^Location: .*token=' "$headers"; then
  echo "redirect leaked the viewer token" >&2
  exit 1
fi
grep -Eiq '^Set-Cookie: imauth_viewer_token=.*; Path=/; HttpOnly; SameSite=Strict\r?$' "$headers"
grep -Eiq '^Referrer-Policy: no-referrer\r?$' "$headers"
grep -Eiq '^Content-Security-Policy: .*frame-ancestors '\''none'\''' "$headers"
grep -Eiq '^X-Content-Type-Options: nosniff\r?$' "$headers"
grep -Eiq '^X-Frame-Options: DENY\r?$' "$headers"
grep -Eiq '^Cache-Control: no-store\r?$' "$headers"

curl -sS -D "$headers" -b "$cookie_jar" -o "$body" \
  "http://127.0.0.1:$port/index.html"
grep -Eq '^HTTP/[^ ]+ 303' "$headers"
grep -Eiq '^Location: /index\.html\?enable_webp=0\r?$' "$headers"
if grep -Eiq '^Set-Cookie:' "$headers"; then
  echo "viewer redirect unexpectedly reset the bootstrap cookie" >&2
  exit 1
fi

curl -sS -D "$headers" -b "$cookie_jar" -o "$body" \
  "http://127.0.0.1:$port/index.html?enable_webp=0"
grep -Fq 'Welcome to nginx!' "$body"
grep -Eiq '^Referrer-Policy: no-referrer\r?$' "$headers"
grep -Eiq '^Content-Security-Policy: .*frame-ancestors '\''none'\''' "$headers"
if grep -Eiq '^Set-Cookie:' "$headers"; then
  echo "viewer content unexpectedly reset the bootstrap cookie" >&2
  exit 1
fi

status=$(curl -sS -o /dev/null -w '%{http_code}' \
  -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
  "http://127.0.0.1:$port/websockify")
[ "$status" = 403 ]

status=$(curl -sS -b "$cookie_jar" -o /dev/null -w '%{http_code}' \
  -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
  "http://127.0.0.1:$port/websockify")
[ "$status" = 404 ]

docker run --rm -d --name "$secure_proxy" --network "$network" \
  -p 127.0.0.1:0:8080 \
  -e "UPSTREAM_HOST=$backend" \
  -e UPSTREAM_PORT=80 \
  -e "IMAUTH_VIEWER_TOKEN=$token" \
  -e IMAUTH_VIEWER_COOKIE_SECURE=Secure \
  -v "$proxy_script:/chrome-proxy-entrypoint.sh:ro" \
  --entrypoint /bin/sh \
  nginx:alpine /chrome-proxy-entrypoint.sh >/dev/null
secure_port=$(docker port "$secure_proxy" 8080/tcp | sed -n 's/.*://p')
curl -sS -D "$headers" -o /dev/null \
  "http://127.0.0.1:$secure_port/index.html?token=$token"
grep -Eiq '^Set-Cookie: imauth_viewer_token=.*; HttpOnly; SameSite=Strict; Secure\r?$' "$headers"

echo "compose and viewer proxy security contract: PASS"
