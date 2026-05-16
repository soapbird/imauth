#!/bin/sh
set -e

# Start Chromium in the background (binds to 127.0.0.1:9223)
/usr/bin/chromium \
  --remote-debugging-port=9223 \
  --remote-allow-origins=http://localhost:50051 \
  --headless=new \
  --disable-gpu \
  --no-sandbox \
  --disable-setuid-sandbox \
  --disable-dev-shm-usage &

CHROME_PID=$!

# Wait for Chromium CDP to be ready
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:9223/json/version >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

# Start a TCP proxy so CDP is reachable on 0.0.0.0:9222 from other containers
python3 -c "
import socket, threading

def forward(a, b):
    try:
        while True:
            d = a.recv(4096)
            if not d:
                break
            b.sendall(d)
    except:
        pass

def handle(client):
    try:
        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.connect(('127.0.0.1', 9223))
        t1 = threading.Thread(target=forward, args=(client, server))
        t2 = threading.Thread(target=forward, args=(server, client))
        t1.start()
        t2.start()
        t1.join()
        t2.join()
    except:
        pass
    finally:
        client.close()

ls = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
ls.bind(('0.0.0.0', 9222))
ls.listen(5)

try:
    while True:
        conn, _ = ls.accept()
        threading.Thread(target=handle, args=(conn,), daemon=True).start()
except KeyboardInterrupt:
    pass
finally:
    ls.close()
" &

PROXY_PID=$!

# Forward termination signals
_cleanup() {
  kill "$PROXY_PID" 2>/dev/null || true
  kill "$CHROME_PID" 2>/dev/null || true
  wait
}
trap _cleanup TERM INT

wait "$CHROME_PID"
kill "$PROXY_PID" 2>/dev/null || true
wait
