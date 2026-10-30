# `exponential-net-wasm`: eNet in the browser

The wasm-bindgen facade over [`enet-core`](../enet-core) for the
Exponential web app. License: MIT OR Apache-2.0.

A browser cannot open UDP sockets, so a browser node is **relay-only**: it
reaches every peer through the relay (a WebSocket), and the traffic is still
end-to-end encrypted (QUIC/TLS 1.3 between the two endpoints' ed25519 keys;
the relay forwards ciphertext). Blobs live in memory.

```js
import init, { WasmNode, generateIdentity } from "exponential-net-wasm";
await init();
const node = await WasmNode.start("https://relay.exponential.at", identityBytes);
const desktop = node.addTicket("enet1…");   // learn the peer
node.allowPeer(desktop);                      // let it in
node.onEvent((e) => { if (e.kind === "chat") show(e.from, e.body); });
await node.sendChat(desktop, new TextEncoder().encode("hi"));
```

API: `WasmNode.start(relayUrl, secretKey)`, `endpointId`, `ticket()`,
`online(timeoutMs)`, `addTicket`, `allowPeer`, `revokePeer`, `setOpen`,
`onEvent(cb)` or `nextEvent()` (one of them), `sendChat(peer, bytes)` →
`{id, timestampMs}`, `joinSpace(space, bootstrap)`, `putRecord(space, key,
value)`, `getRecord`, `records`, `blobAdd(bytes)` → `{hash, size, ticket}`,
`blobFetchTicket(ticket)`, `shutdown()`; free: `generateIdentity()`,
`identitySign(identity, message)` (64-byte ed25519, the install registration),
`pushPublicKey(identity)`, `pushSeal(recipientPublic, plaintext)`,
`pushOpen(recipientSecret, sealed)`. Events are plain objects with a `kind`
(`chat`, `record`, `callIncoming`, `callAccepted`, `callFrame`, `callEnded`,
`peerRefused`); bytes are `Uint8Array`. Calls are not exposed yet (the core
supports them on wasm).

## Build

The web app's build: `scripts/build-enet-wasm.sh` (repo root) → `apps/web/public/enet/`,
loaded lazily after sign-in (`apps/web/src/lib/enet/`); CI runs it in `build-web.yml`
and the smoke (`apps/web/scripts/enet-wasm-smoke.ts`, a wasm node ↔ a native node
through a local `iroh-relay --dev`) in `release-suite.yml`. By hand:

```bash
cd apps/desktop
cargo check -p exponential-net-wasm --target wasm32-unknown-unknown
AR_wasm32_unknown_unknown="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | awk '/^host:/{print $2}')/bin/llvm-ar" \
  cargo build -p exponential-net-wasm --target wasm32-unknown-unknown --profile wasm
wasm-bindgen --target web --out-dir pkg $CARGO_TARGET_DIR/wasm32-unknown-unknown/wasm/exponential_net_wasm.wasm
```

(`wasm-bindgen-cli` must match the `wasm-bindgen` version in `Cargo.lock`.)

**getrandom** is selected per crate, with no workspace `.cargo/config.toml`:
this crate depends, for `wasm32-unknown-unknown` only, on getrandom 0.4 with
`wasm_js` (iroh, eNet) and getrandom 0.2 with `js` (ring, iroh's TLS
crypto). ring's C objects need `llvm-ar` (`rustup component add llvm-tools`):
Apple `ar` writes an empty archive for wasm objects and the link fails on
`ring_core_*` symbols. getrandom 0.3, which would need the `getrandom_backend="wasm_js"`
cfg flag, is not in the graph. ring compiles C for wasm32: the host clang
needs the WebAssembly target (Apple clang and LLVM clang have it).
