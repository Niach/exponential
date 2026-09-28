# VAPP-3 spike: WebRTC data channels (browser ↔ str0m) + the Rust core on iOS/Android

Throwaway spike for #VAPP-3 (decision D3 of #VAPP-1). It produces NUMBERS and a go/no-go, not
product code. Nothing here ships; the branch `exp/VAPP-3` is the reference. Report = a comment
on VAPP-3; `RESULTS.md` is generated from `results/*.jsonl` by `scripts/report.ts`.

## What is being proven

- Web = the browser's own `RTCPeerConnection`; desktop/CLI/iOS/Android = ONE Rust core on str0m
  (`rust/peer-core`), mobile through UniFFI (`rust/peer-ffi`).
- Signaling = the existing steer relay, no relay changes (see "Signaling" below).
- Our own coturn (`coturn/`), forced-relay path included.
- DTLS fingerprints signed with an Ed25519 identity key; tampered SDP is rejected.
- Measurements: direct-success matrix, connect p50/p95, RTT direct vs TURN, 2 MB throughput,
  reconnect after background / network switch, TURN bandwidth per session, binary size + cold start
  on the REAL iOS and Android apps.

## Facts found before building (they change the design in the issue text)

- str0m is 0.24.0 (2026-09-25). Since 0.18 DTLS is pure Rust (`dimpl`), crypto backends
  `rust-crypto` (all platforms) and `apple-crypto` (CryptoKit). No OpenSSL anywhere; the issue's
  4.47 MB baseline (str0m 0.9 + vendored OpenSSL) is obsolete.
- str0m has NO TURN client and does no STUN binding itself: the app gathers srflx/relay candidates
  and pumps relayed traffic through its own TURN client. The spike uses the webrtc-rs `turn` crate
  (tokio) behind the `turn` feature of peer-core.
- Relay: `input.data` (viewer → publisher, ≤ 8 KiB, never logged) and `activity/narration.text`
  (publisher → viewers, ≤ 16 KiB, in the room's replay) carry opaque JSON. The relay never checks
  the DB, so any signed `sessionId` is a room; the publisher must `hello` before viewers `join`.

## Layout and ownership (lanes never touch another lane's directory)

| Path | Owner | Content |
|---|---|---|
| `rust/peer-core` | Lane A | str0m link, gathering, signing, bench protocol (API in `link.rs` is FROZEN) |
| `rust/peer-cli` | Lane A | `daemon` (relay publisher, answers, serves bench, collects results) + `client` (Rust offerer) |
| `rust/peer-ffi` | frozen (main session) | UniFFI facade; lanes B/C consume it, never edit it |
| `web/` | Lane A | `index.html` + `peer.js` + `serve.ts` + `run.ts` (Playwright loop) |
| `coturn/` | main session | compose + conf; `scripts/coturn-up.sh` |
| `scripts/` | main session (report.ts), lanes add `size-*.sh`/`coldstart-*.sh` for their platform | |
| `ios/` | Lane B | `build-xcframework.sh`, `PeerHello/` tuist app, `size-ios.sh`, `coldstart-ios.sh` |
| `android/` | Lane C | `build-so.sh`, `PeerHello/` gradle app, `size-android.sh`, `coldstart-android.sh` |
| `results/` | everyone appends `.jsonl` files named `<lane>-<what>.jsonl` | |
| `apps/ios/Project.swift`, `apps/ios/ExpCore/...` | Lane B only, gated by `PEER_SPIKE=1` | |
| `apps/android/app/build.gradle.kts` + one Kotlin call site | Lane C only, gated by `-PpeerSpike=true` | |

Rust builds ALWAYS use the private target dir: `CARGO_TARGET_DIR=$PWD/spike/vapp-3/rust/target`.

## Signaling envelope (one JSON shape ×4, `rust/peer-core/src/signal.rs`)

```
{"v":1,"from":"<peerId>","to":"<peerId>?","sessionId":"<room>","type":"offer"|"answer","sdp":"...","pub":"<b64url ed25519 pk>","sig":"<b64url>"}
{"v":1,"from":..,"to":..,"sessionId":..,"type":"candidate","candidate":"candidate:...","sdpMid":"0"}
{"v":1,"from":..,"sessionId":..,"type":"end_of_candidates"}
{"v":1,"from":..,"sessionId":..,"type":"hello","pub":"...","version":"..."}        (daemon → room on join)
{"v":1,"from":..,"to":..,"sessionId":..,"type":"result","payload":{...}}          (any → daemon; appended to results/)
{"v":1,"from":..,"to":..,"sessionId":..,"type":"reject","reason":"..."}
{"v":1,"from":..,"sessionId":..,"type":"bye"}
```

Signed bytes (`signing.rs`): `"exp-dtls-fp-v1\n" + hashFunc(lower) + "\n" + fingerprintHex(UPPER, colon-separated) + "\n" + iceUfrag + "\n" + sessionId + "\n" + fromPeerId`.
The receiver extracts `a=fingerprint:` and `a=ice-ufrag:` from the SDP it received, verifies BEFORE
applying it, and checks the DTLS-negotiated remote fingerprint afterwards.

Relay transport: viewers send `{"t":"input","data":"<envelope>"}`; the publisher (daemon) sends
`{"t":"activity","seq":<n>,"event":{"kind":"narration","text":"<envelope>","messageId":"vapp3-<n>"}}`.
Viewers ignore everything that is not a parseable envelope addressed to them (or broadcast).
Roles per room: daemon = publisher, everything else = viewer; one room = one daemon + N peers.

Tickets: `bun spike/vapp-3/scripts/mint-tickets.ts [--secret ..] [--relay ws://localhost:4002] [--ttl 43200] [--session vapp3-xxx]`.
The dev room `vapp3-dev` (7 days) is in `results/.tickets-dev.json` (gitignored; re-mint any time).

## Results line format (`results/*.jsonl`, one JSON object per line)

```
{"kind":"bench","at":"<ISO>","source":"web-chromium|web-webkit|web-safari|ios-sim|ios-device|android-emu|android-device|cli","scenario":"same-machine|same-lan|home-wifi-cellular|home-office-wifi|cellular-cellular|emulator-nat","policy":"all|relay|host","connectMs":0,"rttP50Ms":0,"rttP95Ms":0,"upMbps":0,"downMbps":0,"bodyBytes":2000000,"localPath":"host|srflx|prflx|relay","remotePath":"host|srflx|prflx|relay","ok":true,"error":null,"reconnect":null|{"trigger":"background|network-switch|manual","ms":0},"notes":""}
{"kind":"tamper","at":"<ISO>","source":"...","variant":"sdp|sig|control","rejected":true,"reason":"..."}
{"kind":"size","at":"<ISO>","platform":"ios|android|macos","artifact":"libpeer_ffi.a arm64|libpeer_ffi.so arm64-v8a|Exponential.app|app-production-release.apk lib/arm64-v8a|...","variant":"baseline|minimal-rust-crypto|full-rust-crypto|full-apple-crypto","bytes":0,"notes":""}
{"kind":"coldstart","at":"<ISO>","platform":"ios-sim|ios-device|android-emu|android-device","variant":"baseline|with-core","samplesMs":[0],"medianMs":0,"method":"XCTApplicationLaunchMetric|am start -W TotalTime"}
{"kind":"turn_bandwidth","at":"<ISO>","source":"coturn docker stats","sessionBytesRelayed":0,"seconds":0,"notes":""}
```

## Running the local matrix

1. `./spike/vapp-3/scripts/coturn-native.sh` (coturn NATIVE on the LAN IP via `brew install coturn`, user `exp` / pass `spike` / realm `exponential.local`, UDP 3478 + 49160-49200). The Docker variant (`scripts/coturn-up.sh`) is only usable on a Linux host: on macOS OrbStack/Docker NAT makes coturn see Mac peers as 192.168.107.1, every CREATE_PERMISSION then fails on the reply path (403 Forbidden IP) and the loss stalls SCTP. Lesson for #VAPP-38: coturn needs real source addresses (host networking).
2. Docker dev relay is up on `ws://localhost:4002` (`docker compose --profile steer up -d`); tickets as above.
3. Daemon: `CARGO_TARGET_DIR=$PWD/spike/vapp-3/rust/target cargo run --release --manifest-path spike/vapp-3/rust/Cargo.toml -p peer-cli -- daemon --relay ws://localhost:4002 --ticket <publisher> --session vapp3-dev --turn 192.168.178.71:3478 --turn-user exp --turn-pass spike --turn-realm exponential.local --out spike/vapp-3/results/daemon.jsonl`
4. Web: `bun spike/vapp-3/web/serve.ts` then `bun spike/vapp-3/web/run.ts --runs 20 --policy all` and `--policy relay` (Playwright: chromium headless shell + webkit); Safari: open the printed URL once.
5. Phones: see `ios/README.md` and `android/README.md` (PeerHello apps: Connect · Bench · Reconnect · Send results).
6. `bun spike/vapp-3/scripts/report.ts` regenerates `RESULTS.md`.

## The matrix that needs a person

See `RUNBOOK.md`. Lane reports and gotchas: `ios/README.md`, `android/README.md`.
