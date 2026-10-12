# Sealed node spike: results (#SWM-20)

Run on 2026-10-12, Linux Mint, kernel 6.17.0-42-generic, i9-13900K, QEMU 8.2.2/KVM, Claude Code 2.1.296 (native), Max subscription.
No sudo was needed: the user is in the `kvm` group, which owns `/dev/kvm` and `/dev/vhost-vsock`.
Raw evidence is in [`evidence/`](evidence/); reproduce with [`RUNBOOK.md`](RUNBOOK.md).

**Verdict:** the design works end to end. A real `claude -p` turn with a Bash + git tool call runs inside a NIC-less agent VM that only holds a dummy credential. The real OAuth token lives only in a Hermit unikernel gateway reached over vsock. A scan of the agent VM's whole 2 GiB of RAM finds 0 copies of the token, and 3 in the gateway's RAM (the positive control).
Hermit's vsock stack needed a 73-line kernel patch plus two workarounds to get there, so it is not production-ready yet (see the recommendation).

```
host ── QEMU/KVM ──┬── agent VM  CID 3: Linux 6.17, initramfs, NO NIC, dummy credential
                   │     claude → 127.0.0.1:8080 ─socat─ vsock(2:5000)
                   │                                      │
                   │      host relay (accepts CID 3 only) ┘── vsock(4:8080..8087)
                   │                                                │
                   └── gateway CID 4: Hermit unikernel (Rust, no shell) ┘
                         · token arrives ONCE over vsock from the host (port 9000, then closed)
                         · strips client credentials, injects Bearer + oauth beta
                         · virtio-net (slirp) → rustls → api.anthropic.com:443 only
```

## Q1: does Claude Code work with a subscription behind a credential-swapping proxy?

**Yes, with no special handling beyond swapping one header.** Proved with the host-side build of the same Rust gateway (`gateway/`, `tcp:` listener) before any VM was involved.

- `CLAUDE_CODE_OAUTH_TOKEN=<dummy>` + `ANTHROPIC_BASE_URL=http://127.0.0.1:8787` + `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`: `claude -p "Reply with exactly: PONG"` → `PONG` in 1.8 s. A streamed tool turn (`--output-format stream-json --include-partial-messages`, Bash running `git init/add/commit`) → 2 requests, 32 stream events (22 `content_block_delta`), result correct, commit present.
- **The only endpoint used** in every run (Q1, Q2, Q3) was `POST /v1/messages?beta=true` (plus `/v1/messages/count_tokens` from the curl tests). There were no `/api/oauth/*`, bootstrap or telemetry calls with nonessential traffic disabled, so the gateway's path allowlist is just `/v1/messages`.
- **Headers the real request carries** (OAuth-dummy mode, [`evidence/q1-gateway-oauth-dummy.log`](evidence/q1-gateway-oauth-dummy.log)):
  `Authorization: Bearer <dummy>`, `anthropic-version: 2023-06-01`, `x-app: cli`, `User-Agent: claude-cli/2.1.296 (external, sdk-cli)`, `X-Claude-Code-Session-Id`, `anthropic-dangerous-direct-browser-access: true`, the `X-Stainless-*` set, `Accept-Encoding: gzip, deflate, br, zstd`, and
  `anthropic-beta: claude-code-20250219, oauth-2025-04-20, interleaved-thinking-2025-05-14, thinking-token-count-2026-05-13, context-management-2025-06-27, prompt-caching-scope-2026-01-05, mid-conversation-system-2026-04-07, per-turn-control-2026-07-01, mid-conversation-tool-changes-2026-07-01, effort-2025-11-24, dangerous-tool-use-2026-09-03, afk-mode-2026-01-31, extended-cache-ttl-2025-04-11`.
- With an OAuth-shaped dummy, the client already sends `oauth-2025-04-20` and the subscription betas itself, so the gateway only replaces `Authorization`. It also guarantees the oauth beta is present and strips `x-api-key`, `cookie` and `proxy-authorization`.
- `ANTHROPIC_API_KEY=<dummy>` also works: the client sends `x-api-key`, and the gateway strips it and appends the oauth beta. In that mode, though, the client believes it is an API-key user: it drops `extended-cache-ttl-2025-04-11` and uses API-key defaults ([`evidence/q1-gateway-apikey-dummy.log`](evidence/q1-gateway-apikey-dummy.log)). **Use the OAuth-shaped dummy.**
- The client never validates the dummy locally in `-p` mode. A future client that fetched the profile first would need `/api/oauth/profile` allowlisted; that call returns account PII but not the token.

## Q2: a Rust gateway as a unikernel with outbound TLS

**Hermit works** (Unikraft was not needed and was not tried). The gateway boots, receives the token over vsock, and makes reliable rustls connections to api.anthropic.com with SSE passing through intact.

| | |
|---|---|
| image | **5.5 MB** (kernel + gateway, one ELF, stripped; LTO off) + 131 KB loader (`hermit-loader-x86_64-multiboot` v0.5.7). The host build of the same source is 1.5 MB. |
| boot | Hermit enters the app at **~0.15 s**; QEMU start → token provisioned → listening in **0.17–0.20 s** |
| memory | **40 MiB guest minimum** (8 parallel streams OK; 32 MiB fails with "Not enough memory"). QEMU RSS is ~91 MB at 40 MiB. |
| reliability | **30/30** sequential streamed requests, mean TTFB 0.92 s (max 1.92 s); the host gateway with ring gets 30/30 at 1.09 s. Model latency dominates, so there is no measurable TLS penalty. **16/16** parallel streams of 100 numbers: every `data:` line parses, each starts with `message_start` and ends with `message_stop`, and 0 are malformed. |
| shell | None. Built **without** Hermit's `shell` feature: no interpreter, no filesystem, one address space; the gateway binary *is* the kernel. |

What it took (each is a finding):

1. **The `hermit` crate is no longer on crates.io** (0.13.0 is a `compile_error!` stub). Use git tag `hermit-0.13.2` with `-Zbuild-std` and the pinned `nightly-2026-02-01`.
2. **ring does not link for `x86_64-unknown-hermit`.** Its build script ships no assembly for `target_os = "hermit"`, which leaves undefined `ring_core_0_17_14__*` symbols plus `__stack_chk_fail`. **rustls-rustcrypto 0.0.2-alpha (pure Rust) works**, and it is also what Hermit's own `tls` example uses. Being an alpha crypto provider is a real caveat.
3. **LTO must be off** (`failed to get bitcode from object file`): the kernel archive carries no bitcode.
4. **The `smp` feature panics at boot** (`Could not map Page(0x8000) … ParentEntryHugePage`, with 1 or 2 vCPUs, loader v0.5.7). The gateway runs single-core, which is plenty for a credential-swapping proxy.
5. **virtio-net and virtio-vsock on the same INTx line (11) kill vsock RX.** Pinning the vsock device to PCI slot 5 (`addr=0x5` → IRQ 10) fixes it.
6. **Hermit's vsock is immature.** It holds one connection per bound port (`accept` aliases the listener; a second `accept` → `EBADF`, and there is no backlog). In the [patch](hermit/0001-vsock-close-notify-4tuple-demux-rst.patch) I fixed:
   - it **never notifies the peer on close**, so the host never sees EOF;
   - it **demuxes by local port + peer CID only**, so a stale SHUTDOWN of the previous connection kills the next one on the same port (the first 30-request loop died at request 3 with `EIO`);
   - it **silently drops requests to unbound/busy ports** (the peer times out instead of getting RST).

   I worked around two more: concurrency comes from a **pool of 8 ports**, re-bound after each connection, with the host relay handing out free ports; and since a vsock request that arrives **during boot wedges vsock RX for good**, the host provisions only after the gateway prints its ready line.
7. Token provisioning: the gateway listens once on vsock 9000 and the host pushes the token. I made the gateway the server on purpose: a host-side vsock listener would be reachable from the agent VM too, while the gateway CID is not.

## Q3: vsock wiring, end to end

**Yes.** The agent VM boots the Ubuntu 6.17 generic kernel with an initramfs built from this host's binaries (busybox, bash, claude 2.1.296, git, socat, curl + libs, and the 3 vsock modules; 138 MB gzip). QEMU runs with `-nodefaults`, so there is **no network device at all**, only `lo` and `vhost-vsock-pci` (CID 3). `socat TCP-LISTEN:8080,bind=127.0.0.1,fork VSOCK-CONNECT:2:5000` is the only bridge.

- Boot: QEMU start → init ready in **1.76 s** (guest uptime 1.52 s). Agent QEMU RSS ~770 MB at 2 GiB.
- `claude -p` with `--allowedTools Bash` ran `git init && … git commit && sleep 15 && git log --oneline | wc -l` inside the guest: **exit 0**, 2 API requests through the gateway, 56 stream events (46 deltas), `"result":"1"`, and the commit `first` exists in the guest repo. The 21 s wall time includes the 15 s sleep that opens the RAM-scan window. ([`evidence/q3-q4-agent-serial.log`](evidence/q3-q4-agent-serial.log), [`evidence/q3-gateway.log`](evidence/q3-gateway.log))
- vhost-vsock routes only guest↔host, so **VM↔VM needs a host splice**. `host/vsock-relay.py` accepts only CID 3 and only dials CID 4's pool. `vhost-device-vsock` (rust-vmm, sibling-VM forwarding) could remove the relay; I did not try it.

## Q4: negative tests (all 16 PASS, run inside the agent VM)

**(a) No route out except the gateway.** `/sys/class/net` = `lo` only, and the routing table is empty. Direct `curl http://1.1.1.1` and `https://160.79.104.10` (api.anthropic.com) fail with `Couldn't connect`, and DNS fails with `Could not resolve host`. vsock to the gateway CID 4 directly (8080 and the token port 9000) times out, because vhost-vsock never routes guest→guest. vsock to host ports 9000/22 → `Connection reset by peer`. The relay's 5000 is the only host vsock service.

**(b) The gateway refuses anything but its allowlist** (it logs `REFUSED`):

| request | result |
|---|---|
| `Host: example.com` | 403 `host is not allowlisted` |
| absolute-form proxy request `GET http://example.com/` | 403 `only origin-form` |
| `CONNECT example.com:443` | 405 |
| `/v1/files`, `/api/oauth/profile` | 403 `path is not allowlisted` |
| `Transfer-Encoding: chunked` body | 411 |
| bare CR inside a header value (header-splitting attempt) | 400 |
| `/v1/messages/count_tokens` (allowlisted) | 200 `{"input_tokens":10}` |

The upstream host is fixed per route in code; the client can never name one.

**(c) The token never appears in the guest.**
- Every process environment holds only the dummy (`sk-ant-oat01-DUMMY-…` ×4).
- A filesystem grep for `sk-ant-…` across all 73 guest files, including `~/.claude` transcripts (= every response body), finds only the dummy in `/guest-tests.sh` and the planted `/tmp/control.txt` (positive control). There is no `.credentials.json`.
- **Host-side scan of the full guest RAM** (file-backed `memory-backend-file`, 2 GiB, every page) for the exact 108-byte token and a 24-byte middle slice ([`evidence/q4c-ram-scan.log`](evidence/q4c-ram-scan.log)):

| when | real token | dummy (control) |
|---|---|---|
| agent RAM, mid-turn (claude alive, inside its Bash call) | **0 / 0** | 11 / 12 |
| agent RAM, after all tests | **0 / 0** | 39 / 40 |
| gateway RAM (positive control) | **3 / 3** | – |

- On the host: 167 log, evidence and transcript files from every run contain 0 copies of the token. The gateway only ever logs `token_len=108`.

## Recommended node architecture for #SWM-15

1. **Two VMs per node, as tested.** The agent VM gets no NIC, vsock only, and a dummy OAuth credential (`CLAUDE_CODE_OAUTH_TOKEN`). The gateway VM holds every credential and does all egress. The host stays trusted and owns the relay plus provisioning.
2. **The gateway is one Rust codebase with per-route credential injection.** Today that is `/v1/messages` → api.anthropic.com + OAuth bearer. Next come GitHub smart-HTTP (injecting a short-lived GitHub-App installation token, so git push/fetch go through the gateway) and the swarm instance's `/api/mcp` (injecting the `expu_` key). Same pattern: the client never names a host, and the gateway strips client credentials and injects its own.
3. **Gateway runtime: ship the same source as a static Rust PID 1 in a minimal Linux microVM first, and move to Hermit once its vsock is fixed upstream.** There is no shell in either case: the rootfs is one binary, with no busybox or `/bin/sh`. The Linux variant gets mature vsock, ring/aws-lc and SMP today; Hermit gets the smaller kernel surface after the patch here is upstreamed, the boot race and port model are fixed, and rustls-rustcrypto or a ring hermit port matures. The source already builds for both (`tcp:`/`vsock:` on Linux, `vsock-pool:` on Hermit).
4. **Token: a long-lived `claude setup-token` token per node**, pushed by the host over the one-shot vsock channel at boot, held in gateway RAM only (not file-backed in production; the file backing here was for the scan). That avoids putting a refresh token and a token endpoint into the gateway.
5. **Belt and braces on egress:** put the gateway's NIC in a host netns/nftables jail that allows only the allowlisted upstream IPs on :443. Today slirp lets the gateway dial anything, so the gateway's code is the only allowlist.
6. **Budget the confused deputy.** A compromised agent can still spend the subscription through `/v1/messages`. Enforce a model allowlist, a `max_tokens` cap, and per-node request and token budgets in the gateway (it already parses every request head; budgets need a body peek).

## Open problems

- Hermit: upstream the vsock patch, plus the boot-time RX wedge, the one-connection-per-port model, the `smp` boot panic and the shared-IRQ problem; ring has no hermit asm; rustls-rustcrypto is alpha.
- Short-lived OAuth access tokens expire (this run's token had ~2 h left), so refresh is unsolved unless a `setup-token` token is used.
- Interactive (non `-p`) Claude Code and other features (web fetch, MCP, plugins, auto-update) were not exercised. Each new endpoint is an allowlist decision; WebFetch/WebSearch would need their own policy.
- The gateway speaks HTTP/1.1 with one TLS connection per request (`connection: close`) and refuses chunked request bodies. That is fine for Claude Code; an upstream keep-alive pool would save a handshake per request.
- Agent VM image: built from host glibc binaries for the spike. Production wants a reproducible image (e.g. Nix) and a persistent worktree disk (virtio-blk/virtiofs).
- `vhost-device-vsock` sibling forwarding (to drop the host relay) is untested.
