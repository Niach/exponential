#!/bin/bash
# Boot a sealed node on this host and run the end-to-end + negative tests:
#
#   agent VM (Linux, NO NIC, dummy credential)  CID 3
#     └ 127.0.0.1:8080 ─socat─ vsock 2:5000
#                                 └ host relay (accepts CID 3 only)
#                                     └ vsock 4:8080..8087 (port pool)
#   gateway unikernel (Hermit, holds the token) CID 4 ── virtio-net ── api.anthropic.com
#
# Both VMs get file-backed guest RAM under /dev/shm so the host can scan
# every byte of guest memory for the token (removed at the end).
#
# env: WORK (default /tmp/sealed), TOKEN_FILE (default $WORK/token),
#      GW_IMAGE, LOADER, KERNEL, INITRAMFS, AGENT_MEM (2G), GW_MEM (64M)
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
WORK=${WORK:-/tmp/sealed}
TOKEN_FILE=${TOKEN_FILE:-$WORK/token}
GW_IMAGE=${GW_IMAGE:-$WORK/target/x86_64-unknown-hermit/release/sealed-gateway}
LOADER=${LOADER:-$WORK/dl/loader/hermit-loader-x86_64-multiboot}
KERNEL=${KERNEL:-$WORK/kernel/vmlinuz}
INITRAMFS=${INITRAMFS:-$WORK/q3/agent-initramfs.cpio.gz}
AGENT_MEM=${AGENT_MEM:-2G}
GW_MEM=${GW_MEM:-64M}
AGENT_CID=3 GW_CID=4 RELAY_PORT=5000 POOL_BASE=8080 POOL_N=8
run=$WORK/run-$(date +%Y%m%d-%H%M%S)
mkdir -p "$run"
pids=()
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null; done
  for f in "$run"/*.pid; do [ -f "$f" ] && kill "$(cat "$f")" 2>/dev/null; done
  sleep 0.3
  rm -f /dev/shm/sealed-gw-ram /dev/shm/sealed-agent-ram
}
trap cleanup EXIT
ts() { date +%s.%N; }
since() { awk "BEGIN{printf \"%.3f\", $(ts) - $1}"; }

echo "[node] run dir $run"
# 1. Gateway unikernel. No shell, no disk; token arrives once over vsock.
t0=$(ts)
qemu-system-x86_64 -enable-kvm -cpu host -smp 1 -nodefaults -display none \
  -object memory-backend-file,id=mem,size=$GW_MEM,mem-path=/dev/shm/sealed-gw-ram,share=on \
  -machine pc,memory-backend=mem -m "$GW_MEM" \
  -serial file:"$run/gateway-serial.log" \
  -kernel "$LOADER" -initrd "$GW_IMAGE" \
  -append "-- vsock-pool:$POOL_BASE:$POOL_N vsock-once:9000" \
  -netdev user,id=u0 -device virtio-net-pci,netdev=u0,disable-legacy=on,addr=0x3 \
  -device vhost-vsock-pci,guest-cid=$GW_CID,addr=0x5 \
  -pidfile "$run/gateway-qemu.pid" &
python3 "$here/host/provision-token.py" "$TOKEN_FILE" $GW_CID "$run/gateway-serial.log" 9000 | tee "$run/provision.log"
echo "[node] gateway up + token provisioned in $(since "$t0")s"

# 2. Host relay: the agent's vsock -> the gateway's port pool.
python3 "$here/host/vsock-relay.py" vsock:$RELAY_PORT:$AGENT_CID $GW_CID $POOL_BASE $POOL_N > "$run/relay.log" 2>&1 &
pids+=($!)

# 3. Agent VM: Linux, NO network device (-nodefaults), vsock only.
t1=$(ts)
qemu-system-x86_64 -enable-kvm -cpu host -smp 4 -nodefaults -display none \
  -object memory-backend-file,id=mem,size=$AGENT_MEM,mem-path=/dev/shm/sealed-agent-ram,share=on \
  -machine pc,memory-backend=mem -m "$AGENT_MEM" \
  -serial file:"$run/agent-serial.log" \
  -kernel "$KERNEL" -initrd "$INITRAMFS" \
  -append "console=ttyS0 quiet panic=-1 HOLD_SECONDS=20" \
  -device vhost-vsock-pci,guest-cid=$AGENT_CID \
  -no-reboot -pidfile "$run/agent-qemu.pid" &
agent_qemu=$!

wait_for() { # <file> <marker> <timeout_s>
  local end=$(( $(date +%s) + $3 ))
  until grep -aq "$2" "$1" 2>/dev/null; do
    [ "$(date +%s)" -gt "$end" ] && return 1
    sleep 0.1
  done
}
wait_for "$run/agent-serial.log" "AGENT VM UP" 60 && echo "[node] agent VM up in $(since "$t1")s (guest: $(grep -ao 'AGENT VM UP [0-9.]*s' "$run/agent-serial.log"))"

# 4. Scan guest RAM while claude is mid-turn (its Bash tool opens a window).
if wait_for "$run/agent-serial.log" "SCAN-WINDOW-OPEN" 240; then
  echo "[node] scan window open (claude mid-turn):"
  python3 "$here/host/scan-ram.py" /dev/shm/sealed-agent-ram real-token="$TOKEN_FILE" \
    dummy-credential=@sk-ant-oat01-DUMMY-guest-credential-not-a-real-token | tee -a "$run/scan.log"
fi
wait_for "$run/agent-serial.log" "GUEST TESTS DONE" 300
echo "[node] guest tests done; final scans:"
python3 "$here/host/scan-ram.py" /dev/shm/sealed-agent-ram real-token="$TOKEN_FILE" \
  dummy-credential=@sk-ant-oat01-DUMMY-guest-credential-not-a-real-token | tee -a "$run/scan.log"
echo "[node] positive control: the gateway's RAM must contain the token:"
python3 "$here/host/scan-ram.py" /dev/shm/sealed-gw-ram real-token="$TOKEN_FILE" | tee -a "$run/scan.log"
echo "[node] RSS: gateway $(ps -o rss= -p "$(cat "$run/gateway-qemu.pid")")KiB agent $(ps -o rss= -p "$(cat "$run/agent-qemu.pid")")KiB" | tee -a "$run/scan.log"
wait "$agent_qemu"
echo "[node] gateway log:"; grep -a '\[gateway\]' "$run/gateway-serial.log"
echo "[node] relay log:"; cat "$run/relay.log"
echo "[node] logs in $run"
