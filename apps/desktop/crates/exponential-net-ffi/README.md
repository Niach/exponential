# `exponential-net-ffi`: eNet for Swift and Kotlin

The UniFFI 0.31 facade over [`enet-core`](../enet-core) for iOS,
macOS and Android: an xcframework / an Android `.so` plus the generated
bindings. License: MIT OR Apache-2.0.

## The contract

- `EnetNode.start(config, identity)` (async; `identity` = 32 bytes from
  `generateIdentity()`, kept in the keychain). `defaultConfig()` = our relay
  (`https://relay.exponential.at`), n0 relays off, nobody allowed. The facade
  owns one tokio runtime per process (2 worker threads, `enet`); every async
  method runs there and is awaited from Swift concurrency / Kotlin
  coroutines.
- Addressing and access: `endpointId()`, `ticket()` (`enet1…`, for a QR code),
  `addTicket(ticket)` (→ the peer's id), `allowPeer` / `revokePeer` /
  `allowedPeers`, `setOpen(bool)` (pairing window), `setAccessPolicy(policy?)`
  (foreign `EnetAccessPolicy.allow(peer, alpn)`), `online(timeoutMs)`.
- Events: a foreign `EnetListener.onEvent(event)` (`setListener`, called on an
  eNet thread) OR `await nextEvent()` in a loop; one per node. `EnetEvent` =
  `chat {from, message}`, `record {record}`, `callIncoming {call, from}`,
  `callAccepted {call}`, `callFrame {call, seq, data}`, `callEnded {call,
  reason: local|remote|failed: …}`, `peerRefused {peer, alpn}`.
- Chat: `sendChat(peer, body)` → `EnetChatMessage {id, timestampMs, body}`
  once the peer acknowledged it.
- Files: `blobAdd(data)`, `blobAddPath(path)` → `EnetBlob {hash, hashHex,
  size, ticket}`; `blobGet(hash)`, `blobFetch(peer, hash)`,
  `blobFetchTicket(ticket)`. `EnetConfig.blobDir` persists blobs (feature
  `fs-store`, default on).
- Records: `joinSpace(space[32], bootstrap)` (= subscribe: records arrive as
  `record` events), `putRecord(space, key, value)`, `getRecord`, `records`,
  `leaveSpace`.
- Calls: `startCall(peer)` → call id, `acceptCall`, `sendFrame(call, opus)` →
  seq, `hangUp`.
- `shutdown()`.
- Free functions: `generateIdentity`, `endpointIdOf`, `identitySign(identity,
  message)` (64-byte ed25519; the install registration signature, vector in
  `packages/domain-contract/fixtures/install-register.json`), `parseTicket`,
  `pushKeypairDerive(identity)` / `pushKeypairGenerate` (`{secret, public}`),
  `pushSeal(recipientPublic, plaintext)`, and **`pushOpen(recipientSecret,
  sealed)`** for the iOS Notification Service Extension / the Android FCM
  service: no node, no runtime, only the X25519 secret (byte format in the
  core README).
- Errors: `EnetError` = `Invalid | Bind | Connect | Refused | Crypto |
  NotFound | Transport | Closed | Panic`, each with a `reason`. Async work runs
  in spawned tasks, so a Rust panic comes back as `Panic` instead of crossing
  the FFI.

## Build

```bash
bash apps/desktop/crates/exponential-net-ffi/generate-bindings.sh  # bindings/swift + bindings/kotlin from the host library
bash apps/desktop/crates/exponential-net-ffi/build-ios.sh          # out/ExponentialNetFFI.xcframework (+ bindings)
bash apps/desktop/crates/exponential-net-ffi/build-android.sh      # out/jniLibs/<abi>/libexponential_net_ffi.so (+ bindings)
bash apps/desktop/crates/exponential-net-ffi/run-binding-tests.sh  # the Swift suite against the host dylib
```

### Push-only build (EXP-1435)

`ENET_PUSH_ONLY=1` on any of the three scripts builds the facade with
`--no-default-features`: `enet-core` without its `node` feature, so
only `pushOpen`, `pushSeal`, `pushKeypairGenerate`/`Derive` and `EnetError`
remain, on RustCrypto alone (no iroh, no tokio). It is what the Exponential
apps link (the iOS Notification Service Extension has a ~24 MB memory cap):
bindings under `bindings/push/{swift,kotlin}` (committed), artifacts under
`out/push/` (`ExponentialNetFFI.xcframework`, `jniLibs/<abi>/libexponential_net_ffi.so`).
Same module/package/library names as the full build, so an app links one or
the other, never both. `tests/push_fixture.rs` replays
`packages/domain-contract/fixtures/push-seal.json` in both builds
(`cargo test -p exponential-net-ffi --no-default-features`).

Device builds use the workspace's `mobile` profile (LTO, one codegen unit,
stripped), one crate type per build so LTO applies. The generated binding
sources under `bindings/` are COMMITTED (the contract, reviewable without a
toolchain); `out/` is ignored. Android needs the NDK (`ANDROID_NDK_HOME` or
`~/Library/Android/sdk/ndk/*`): ring, iroh's TLS crypto, compiles C, so a
bare `cargo check --target aarch64-linux-android` fails without the NDK
clang; `cargo ndk -t arm64-v8a check -p exponential-net-ffi` works. Kotlin
uses JNA (≥ 5.12), like `exponential-ui-ffi`.

`tests/facade.rs` drives two nodes over localhost from Rust (chat, records,
blobs, a call, a listener, refusal by a foreign policy, push sealing);
`tests/swift/main.swift` does chat, records and push sealing through the
Swift binding.
