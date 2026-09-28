#!/usr/bin/env bash
# Run coturn natively on the Mac's LAN IP (brew install coturn). Stops the docker variant first.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
lan="${LAN_IP:-$(ipconfig getifaddr en0)}"
conf="$here/coturn/turnserver-native.conf"
sed -i.bak -E "s/^(listening-ip|relay-ip|external-ip)=.*/\1=${lan}/" "$conf" && rm -f "$conf.bak"
docker compose -f "$here/coturn/docker-compose.yaml" down >/dev/null 2>&1 || true
pkill -f "turnserver -c $conf" >/dev/null 2>&1 || true
nohup turnserver -c "$conf" > /tmp/vapp3-coturn.stdout 2>&1 &
echo "native coturn pid $! on turn:${lan}:3478 (exp/spike, realm exponential.local); log /tmp/vapp3-coturn.log"
