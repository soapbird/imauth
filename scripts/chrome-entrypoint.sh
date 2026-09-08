#!/bin/bash
set -e

mkdir -p /home/kasm-user/.config/chromium
rm -f /home/kasm-user/.config/chromium/SingletonLock /home/kasm-user/.config/chromium/SingletonSocket /home/kasm-user/.config/chromium/SingletonCookie

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

exec python3 /scripts/chrome-runtime.py
