---
title: eNet guides
description: Add enet-core to a Rust app, start a node and send a chat message to a peer.
---

## Install

The core is on crates.io:

```sh
cargo add enet-core
```

The Swift, Kotlin and WebAssembly bindings are not published yet; until they are, build `exponential-net-ffi` or `exponential-net-wasm` from the repo ([ffi](/docs/libraries/enet/reference/ffi/), [wasm](/docs/libraries/enet/reference/wasm/)).

## Start a node and say hi

```rust
let node = Node::start(NetConfig::default(), Identity::from_bytes(saved)).await?;
let mut events = node.events().unwrap();          // taken once
node.add_ticket(&"enet1…".parse()?);              // learn a peer's addresses
node.access().allow(peer_id);                     // and let it in
node.send_chat(peer_id, b"hi".to_vec()).await?;   // acked by the peer
```

- Keep the `Identity` (an ed25519 secret key, 32 bytes) across restarts; its public half is the id peers dial and allowlist.
- Hand peers your `Ticket` (`enet1…`, fits a QR code) so they can find you.
- `NetConfig::relay` picks the relay: the default, your own URLs, or `Disabled` for direct addresses only.

Every channel, push sealing and the security model: [Reference](/docs/libraries/enet/reference/).
