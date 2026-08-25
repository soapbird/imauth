#!/bin/bash
set -e

mkdir -p /home/kasm-user/.config/chromium
rm -f /home/kasm-user/.config/chromium/SingletonLock /home/kasm-user/.config/chromium/SingletonSocket /home/kasm-user/.config/chromium/SingletonCookie
chown -R kasm-user:kasm-user /home/kasm-user/.config /home/kasm-user/.cache /home/kasm-user/.local /home/kasm-user/Desktop /home/kasm-user/Downloads /home/kasm-user/Uploads 2>/dev/null || true

mkdir -p /home/kasm-user/.vnc
cat > /home/kasm-user/.vnc/kasmvnc.yaml <<'YAML'
encoding:
  max_frame_rate: 24
  full_frame_updates: none
  rect_encoding_mode:
    min_quality: 4
    max_quality: 6
    consider_lossless_quality: 10
    rectangle_compress_threads: auto
  compare_framebuffer: auto
YAML

chown -R kasm-user:kasm-user /home/kasm-user/.config /home/kasm-user/.cache /home/kasm-user/.local /home/kasm-user/Desktop /home/kasm-user/Downloads /home/kasm-user/Uploads /home/kasm-user/.vnc 2>/dev/null || true

printf '#!/usr/bin/env bash\nexit 0\n' >/usr/bin/desktop_ready
chmod +x /usr/bin/desktop_ready

cat > /tmp/browser-cdp-relay.py <<'PY'
import os
import shutil
import socket
import socketserver
import threading

class Relay(socketserver.BaseRequestHandler):
    def handle(self):
        try:
            upstream = socket.create_connection(("127.0.0.1", 9222), timeout=10)
        except OSError:
            return

        def pump(src, dst):
            try:
                shutil.copyfileobj(src.makefile("rb", buffering=0), dst.makefile("wb", buffering=0))
            except OSError:
                pass
            finally:
                for item in (src, dst):
                    try:
                        item.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass

        threads = [
            threading.Thread(target=pump, args=(self.request, upstream), daemon=True),
            threading.Thread(target=pump, args=(upstream, self.request), daemon=True),
        ]
        for thread in threads:
            thread.start()
        try:
            for thread in threads:
                thread.join()
        finally:
            for item in (self.request, upstream):
                try:
                    item.close()
                except OSError:
                    pass

class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

bind_address = os.environ.get("CDP_RELAY_BIND_ADDR") or socket.gethostbyname(socket.gethostname())
Server((bind_address, 9223), Relay).serve_forever()
PY

runuser -u kasm-user -- python3 /tmp/browser-cdp-relay.py &
exec runuser -u kasm-user -- /dockerstartup/kasm_default_profile.sh /dockerstartup/vnc_startup.sh /dockerstartup/kasm_startup.sh --wait
