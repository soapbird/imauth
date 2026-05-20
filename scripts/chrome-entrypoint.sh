#!/bin/sh
set -e

# --- Configuration ---
VNC_PASSWORD="${IMAUTH_VNC_PASSWORD:-imauth}"
DISPLAY_NUM="${DISPLAY_NUM:-99}"
DISPLAY=":${DISPLAY_NUM}"

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
  --remote-debugging-port=9222 \
  --remote-debugging-address=0.0.0.0 \
  --remote-allow-origins=* \
  --no-sandbox \
  --disable-setuid-sandbox \
  --disable-dev-shm-usage \
  --disable-gpu \
  --window-size=390,844 \
  --display="$DISPLAY" &

CHROME_PID=$!

# Wait for Chromium CDP to be ready
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:9222/json/version >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

# Forward termination signals
_cleanup() {
  kill "$CHROME_PID" 2>/dev/null || true
  kill "$WEBSOCKIFY_PID" 2>/dev/null || true
  kill "$X11VNC_PID" 2>/dev/null || true
  kill "$XVFB_PID" 2>/dev/null || true
  wait
}
trap _cleanup TERM INT

wait "$CHROME_PID"
kill "$WEBSOCKIFY_PID" 2>/dev/null || true
kill "$X11VNC_PID" 2>/dev/null || true
kill "$XVFB_PID" 2>/dev/null || true
wait
