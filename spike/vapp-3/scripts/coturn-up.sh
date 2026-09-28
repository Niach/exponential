#!/usr/bin/env bash
# (Re)start the spike coturn with the current LAN IP as external-ip.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
lan="${LAN_IP:-$(ipconfig getifaddr en0 2>/dev/null || hostname -I | awk '{print $1}')}"
sed -i.bak -E "s/^external-ip=.*/external-ip=${lan}/" "$here/coturn/turnserver.conf" && rm -f "$here/coturn/turnserver.conf.bak"
docker compose -f "$here/coturn/docker-compose.yaml" up -d
echo "coturn up: turn:${lan}:3478 (user exp / pass spike / realm exponential.local)"
