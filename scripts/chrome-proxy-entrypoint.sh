#!/bin/sh
set -e

cat > /etc/nginx/conf.d/default.conf <<EOF
server {
  listen 8080;
  server_name _;

  proxy_http_version 1.1;
  tcp_nodelay on;
  proxy_buffering off;
  proxy_request_buffering off;
  proxy_set_header Host \$host;
  proxy_set_header X-Real-IP \$remote_addr;
  proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
  proxy_set_header X-Forwarded-Proto http;
  proxy_set_header Upgrade \$http_upgrade;
  proxy_set_header Connection \$connection_upgrade;
  proxy_read_timeout 86400;
  proxy_send_timeout 86400;

  location / {
    proxy_pass http://${UPSTREAM_HOST}:${UPSTREAM_PORT};
  }
}
map \$http_upgrade \$connection_upgrade {
  default upgrade;
  '' close;
}
EOF

exec nginx -g 'daemon off;'
