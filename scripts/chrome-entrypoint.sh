#!/bin/sh
set -e

# --- Configuration ---
VNC_PASSWORD="${IMAUTH_VNC_PASSWORD:-imauth}"
DISPLAY_NUM="${DISPLAY_NUM:-99}"
DISPLAY=":${DISPLAY_NUM}"

# Chromium 131+ binds CDP to 127.0.0.1 regardless of
# --remote-debugging-address. We run Chrome on 9223 (localhost only)
# and use socat to expose 9222 on all interfaces so cross-container
# connections work.
CHROME_CDP_PORT=9223
SOCAT_CDP_PORT=9222

# --- Start Xvfb (virtual framebuffer) ---
Xvfb "$DISPLAY" -screen 0 1280x1024x24 &
XVFB_PID=$!

# Wait for Xvfb to start
for i in $(seq 1 10); do
  if [ -e "/tmp/.X11-unix/X${DISPLAY_NUM}" ]; then
    break
  fi
  sleep 0.5
done

# --- Start x11vnc ---
x11vnc -display "$DISPLAY" -forever -noxdamage -repeat -passwd "$VNC_PASSWORD" -shared &
X11VNC_PID=$!

# --- Start noVNC (websockify) ---
# Try both common noVNC static file paths
NOVNC_WEB_DIR="/usr/share/novnc"
if [ ! -d "$NOVNC_WEB_DIR" ]; then
  NOVNC_WEB_DIR="/usr/share/noVNC"
fi
websockify --web="$NOVNC_WEB_DIR" 6080 localhost:5900 &
WEBSOCKIFY_PID=$!

# --- Start Chromium in headed mode on Xvfb ---
/usr/bin/chromium \
  --remote-debugging-port=${CHROME_CDP_PORT} \
  --remote-debugging-address=0.0.0.0 \
  --remote-allow-origins=* \
  --no-sandbox \
  --disable-setuid-sandbox \
  --disable-dev-shm-usage \
  --disable-gpu \
  --window-size=390,844 \
  --display="$DISPLAY" &

CHROME_PID=$!

# Wait for Chromium CDP to be ready on its internal port
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:${CHROME_CDP_PORT}/json/version >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

# --- Start socat to forward external 9222 → internal 9223 ---
socat TCP-LISTEN:${SOCAT_CDP_PORT},fork TCP:127.0.0.1:${CHROME_CDP_PORT} &
SOCAT_PID=$!

# Verify socat is working
for i in $(seq 1 10); do
  if curl -sf http://127.0.0.1:${SOCAT_CDP_PORT}/json/version >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

# Forward termination signals
_cleanup() {
  kill "$SOCAT_PID" 2>/dev/null || true
  kill "$CHROME_PID" 2>/dev/null || true
  kill "$WEBSOCKIFY_PID" 2>/dev/null || true
  kill "$X11VNC_PID" 2>/dev/null || true
  kill "$XVFB_PID" 2>/dev/null || true
  wait
}
trap _cleanup TERM INT

wait "$CHROME_PID"
kill "$SOCAT_PID" 2>/dev/null || true
kill "$WEBSOCKIFY_PID" 2>/dev/null || true
kill "$X11VNC_PID" 2>/dev/null || true
kill "$XVFB_PID" 2>/dev/null || true
wait
