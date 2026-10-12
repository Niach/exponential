# Sealed node spike: runbook

How to reproduce [`RESULTS.md`](RESULTS.md) from zero on an x86_64 Linux host (tested on Linux Mint, kernel 6.17.0-42-generic). Everything runs **without sudo** as long as your user is in the group that owns `/dev/kvm` and `/dev/vhost-vsock` (`kvm` on Ubuntu/Mint).

## 0. Prerequisites

```bash
ls -l /dev/kvm /dev/vhost-vsock   # both rw for group kvm
groups | grep -w kvm              # else: sudo usermod -aG kvm $USER (re-login)
qemu-system-x86_64 --version      # 8.x (apt: qemu-system-x86)
socat -V | head -1                # with VSOCK support (1.7.4+)
busybox | head -1                 # static busybox (apt: busybox-static)
command -v zstd cpio curl git rustup python3 claude
```

Claude Code must be the native build (`claude --version`); its libs are copied into the agent initramfs. `WORK` below is a scratch dir; `S` is this directory.

```bash
export WORK=/tmp/sealed S=$PWD/spike/sealed-node
mkdir -p $WORK && chmod 700 $WORK
```

## 1. Token (never commit, print or log it)

Best: a long-lived `claude setup-token` token (needs a browser once). For a throwaway test, the access token of an existing Claude Code login works:

```bash
umask 077
python3 -c "import json,os;print(json.load(open(os.path.expanduser(os.environ.get('CLAUDE_CONFIG_DIR','~/.claude'))+'/.credentials.json'))['claudeAiOauth']['accessToken'],end='')" > $WORK/token
stat -c '%a %s' $WORK/token      # 600, ~108 bytes
```

## 2. Q1: host-only proxy, no VMs

```bash
cd $S/gateway
CARGO_TARGET_DIR=$WORK/target cargo build --release
GATEWAY_TOKEN_FILE=$WORK/token GATEWAY_VERBOSE=1 $WORK/target/release/sealed-gateway tcp:127.0.0.1:8787 &
mkdir -p $WORK/q1/cfg && cd $WORK/q1
env -i HOME=$WORK/q1 PATH=/usr/bin:/bin CLAUDE_CONFIG_DIR=$WORK/q1/cfg \
  ANTHROPIC_BASE_URL=http://127.0.0.1:8787 \
  CLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-DUMMY-not-a-real-token \
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
  claude -p "Reply with exactly: PONG" </dev/null
kill %1
```

The gateway log (stderr) shows every header with credentials redacted (`GATEWAY_VERBOSE=1`).

## 3. Boot artifacts

```bash
$S/fetch-deps.sh $WORK            # Hermit loader v0.5.7 + Ubuntu kernel matching `uname -r`
$S/gateway/build-hermit.sh $WORK  # clones hermit-rs@hermit-0.13.2, applies hermit/*.patch, builds the unikernel
$S/agent-vm/build-initramfs.sh $WORK/q3/agent-initramfs.cpio.gz
```

`build-hermit.sh` installs `nightly-2026-02-01` + `rust-src` via rustup on first use; the first build takes about 30 s. The initramfs uses the HOST kernel's vsock modules, which is why the agent kernel must equal `uname -r`.

## 4. Q2: the gateway unikernel alone

```bash
cd $WORK && mkdir -p q2
qemu-system-x86_64 -enable-kvm -cpu host -smp 1 -m 64M -nodefaults -display none \
  -serial file:q2/gw.log \
  -kernel dl/loader/hermit-loader-x86_64-multiboot \
  -initrd target/x86_64-unknown-hermit/release/sealed-gateway \
  -append "-- vsock-pool:8080:8 vsock-once:9000" \
  -netdev user,id=u0 -device virtio-net-pci,netdev=u0,disable-legacy=on,addr=0x3 \
  -device vhost-vsock-pci,guest-cid=4,addr=0x5 &       # addr=0x5: own IRQ line, see RESULTS Q2.5
python3 $S/host/provision-token.py $WORK/token 4 q2/gw.log 9000
python3 $S/host/vsock-relay.py tcp:127.0.0.1:8788 4 8080 8 &   # host TCP -> gateway port pool
curl -sN -H 'content-type: application/json' -H 'anthropic-version: 2023-06-01' \
  --data '{"model":"claude-haiku-5-5","max_tokens":64,"stream":true,"system":"You are Claude Code, Anthropic'"'"'s official CLI for Claude.","messages":[{"role":"user","content":"Count to 10"}]}' \
  http://127.0.0.1:8788/v1/messages
```

You can also point `claude` at `http://127.0.0.1:8788` as in step 2. Stop with `kill %1 %2`. Do not drop `-smp 1`: the `smp` feature panics on boot (RESULTS Q2.4).

## 5. Q3 + Q4: the whole node

```bash
$S/run-node.sh     # env: WORK, TOKEN_FILE, AGENT_MEM=2G, GW_MEM=64M
```

It boots the gateway and pushes the token, starts the relay (`vsock:5000`, CID 3 only), then boots the NIC-less agent VM. The VM runs [`agent-vm/guest-tests.sh`](agent-vm/guest-tests.sh): the claude turn with Bash + git, then the Q4 a/b/c tests. While claude is mid-turn, the script scans the agent's whole RAM for the token, then rescans after the tests and scans the gateway's RAM as the positive control. Expected output: see [`evidence/run-node-output.log`](evidence/run-node-output.log); the in-guest PASS/FAIL list is in `$WORK/run-*/agent-serial.log`. Guest RAM files under `/dev/shm` are deleted on exit.

## 6. Clean up

```bash
rm -f $WORK/token /dev/shm/sealed-*-ram
rm -rf $WORK                      # includes ~2 GB of build output
```

## Files

| path | what |
|---|---|
| `gateway/src/main.rs` | request policy: origin-form only, Host check, path allowlist, credential strip + inject, streaming relay |
| `gateway/src/listen.rs` | `tcp:`, Linux `vsock:`, Hermit `vsock-pool:` listeners; one-shot vsock token receive |
| `gateway/build-hermit.sh` | unikernel build (pinned nightly, build-std, LTO off, patched kernel) |
| `hermit/*.patch` | Hermit kernel vsock fixes: close notification, 4-tuple demux, RST for untakeable requests |
| `host/provision-token.py` | pushes the token once into the gateway over vsock |
| `host/vsock-relay.py` | host splice agent CID → gateway port pool (vhost-vsock has no guest↔guest path) |
| `host/scan-ram.py` | counts token occurrences in a guest RAM file (prints counts only) |
| `agent-vm/` | initramfs builder, init, in-guest tests |
| `run-node.sh`, `fetch-deps.sh` | orchestration |
| `evidence/` | sanitized logs from the recorded runs (no token; dummy credentials only) |
