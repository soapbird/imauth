#!/bin/sh
set -e

case "${IMAUTH_VIEWER_TOKEN:-}" in
  ""|*[!A-Za-z0-9_-]*)
    echo "IMAUTH_VIEWER_TOKEN must contain only letters, digits, '_' or '-'" >&2
    exit 1
    ;;
esac

if [ "${#IMAUTH_VIEWER_TOKEN}" -lt 32 ]; then
  echo "IMAUTH_VIEWER_TOKEN must be at least 32 characters" >&2
  exit 1
fi

case "${IMAUTH_VIEWER_COOKIE_SECURE:-}" in
  "") viewer_cookie_secure="" ;;
  Secure) viewer_cookie_secure="; Secure" ;;
  *)
    echo "IMAUTH_VIEWER_COOKIE_SECURE must be empty or 'Secure'" >&2
    exit 1
    ;;
esac

cat > /etc/nginx/conf.d/default.conf <<EOF
map \$arg_token \$viewer_query_authorized {
  default 0;
  ~^${IMAUTH_VIEWER_TOKEN}\$ 1;
}

map \$cookie_imauth_viewer_token \$viewer_cookie_authorized {
  default 0;
  ~^${IMAUTH_VIEWER_TOKEN}\$ 1;
}

map "\$viewer_query_authorized:\$viewer_cookie_authorized" \$viewer_authorized {
  default 0;
  "1:0" 1;
  "1:1" 1;
  "0:1" 1;
}

map \$viewer_query_authorized \$viewer_set_cookie {
  default "";
  1 "imauth_viewer_token=\$arg_token; Path=/; HttpOnly; SameSite=Strict${viewer_cookie_secure}";
}

upstream viewer_backend {
  server ${UPSTREAM_HOST}:${UPSTREAM_PORT};
}

server {
  listen 8080;
  server_name _;
  access_log off;

  add_header Referrer-Policy "no-referrer" always;
  add_header Content-Security-Policy "default-src 'self'; script-src 'self' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ws: wss:; font-src 'self' data:; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'" always;
  add_header X-Content-Type-Options "nosniff" always;
  add_header X-Frame-Options "DENY" always;
  add_header Cache-Control "no-store" always;
  add_header Permissions-Policy "camera=(), microphone=(), geolocation=()" always;
  add_header Set-Cookie \$viewer_set_cookie always;

  proxy_http_version 1.1;
  tcp_nodelay on;
  proxy_buffering off;
  proxy_request_buffering off;
  proxy_set_header Host \$host;
  proxy_set_header X-Real-IP \$remote_addr;
  proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
  proxy_set_header X-Forwarded-Proto \$scheme;
  proxy_set_header Upgrade \$http_upgrade;
  proxy_set_header Connection \$connection_upgrade;
  proxy_read_timeout 86400;
  proxy_send_timeout 86400;

  location / {
    if (\$viewer_query_authorized = 1) {
      return 303 \$uri;
    }

    if (\$viewer_authorized = 0) {
      return 403;
    }

    proxy_pass http://viewer_backend\$uri;
  }
}
map \$http_upgrade \$connection_upgrade {
  default upgrade;
  '' close;
}
EOF

exec nginx -g 'daemon off;'
