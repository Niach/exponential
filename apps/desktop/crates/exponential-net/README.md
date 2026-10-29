# `exponential-net` (eNet): the networking core

eNet is the p2p, end-to-end encrypted networking library shared by
Exponential and spezl, built on [iroh](https://iroh.computer) 1.x. One Rust
core; Swift and Kotlin get it through [`exponential-net-ffi`](../exponential-net-ffi)
(UniFFI), the browser through [`exponential-net-wasm`](../exponential-net-wasm)
(wasm-bindgen).

License: **MIT OR Apache-2.0** (the three eNet crates, unlike the rest of
this workspace, so an MIT app like spezl can ship them; iroh, iroh-gossip
and iroh-blobs are MIT OR Apache-2.0 too).

## Node

```rust
let node = Node::start(NetConfig::default(), Identity::from_bytes(saved)).await?;
let mut events = node.events().unwrap();          // taken once
node.add_ticket(&"enet1…".parse()?);              // learn a peer's addresses
node.access().allow(peer_id);                     // and let it in
node.send_chat(peer_id, b"hi".to_vec()).await?;   // acked by the peer
```

- `Identity` = an ed25519 secret key (32 bytes; `generate`, `from_bytes`,
  `to_bytes`); its public half is the `EndpointId` peers dial and allowlist.
- `Ticket` = the node's address card for a QR code or link:
  `enet1` + lowercase unpadded base32 of
  `id[32] | u8 n, n × (u8 len, relay URL) | u8 m, m × (u8 4|6, ip, u16 BE port)`.
  Parsing is case-insensitive (the uppercase form fits QR alphanumeric mode).
- `NetConfig`: `relay` = `Custom(urls)` (default
  `https://relay.exponential.at`) or `Disabled` (direct addresses only);
  `n0_relays` (number 0's public relays) is OFF by default;
  `relay_auth_token`, `bind_addr`, `blob_dir` (`fs-store`), `allowed_peers`,
  `allow_all`. `NetConfig::local_only()` = localhost, no relay (tests).
- Address lookup is a `MemoryLookup` fed by tickets (`add_ticket` /
  `add_peer`); no DNS/pkarr publishing, so peers learn each other through
  tickets (QR pairing, contact exchange, records).
- `Access` gates every INCOMING connection, every channel: a peer gets in when
  the node is open (`set_open`, e.g. while a pairing QR code is shown), the
  peer is allowed (`allow`/`revoke`), or the app's hook `(peer, alpn) -> bool`
  says yes. Refusal happens right after the TLS handshake (so the peer id is
  authenticated), closes with QUIC application code 403, raises
  `Event::PeerRefused`, and the dialer gets `NetError::Refused`. Outgoing
  connections are not gated.

## Channels

| channel | ALPN | what |
|---|---|---|
| chat | `exp/enet/chat/1` | reliable, ordered, acknowledged messages |
| files | `/iroh-bytes/4` (`iroh_blobs::ALPN`) | content-addressed blobs, BLAKE3-verified |
| records | `exp/enet/gossip/1` + `exp/enet/records-sync/1` | signed LWW records per space |
| call | `exp/enet/call/1` | opaque audio frames over QUIC datagrams |

**chat**: one bidirectional stream per peer; frames
`[u32 BE len][id:16][timestamp_ms:u64 BE][body]`, max body 1 MiB. The receiver
queues `Event::Chat {from, message}` and answers the 16-byte id; `send_chat`
returns once that ack arrives (one retry on a fresh connection). One stream =
in-order delivery; sends to one peer are serialized.

**files**: `blob_add(bytes)` / `blob_add_path(path)` (native) → `BlobRef {hash,
size, ticket}` (an iroh `blob…` ticket); `blob_fetch(peer, hash)`,
`blob_fetch_ticket(ticket)`, `blob_get(hash)` (local). In-memory store by
default; `--features fs-store` + `NetConfig::blob_dir` persists (native only).

**records** (our own, NOT iroh-docs): `join_space(space, bootstrap)`,
`put_record(space, key, value)`, `get_record`, `records(space)`,
`apply_records(snapshot)`, `sync_space(space, peer)`, `leave_space`. A record =
`{space[32], key (UTF-8 ≤ 1 KiB), value (≤ 48 KiB), author, clock, signature}`,
signed by the author's node key and verified by every receiver (a bad
signature is never applied). Per key the last writer wins: the larger
`(clock, author, value)` tuple, where `clock` = a hybrid logical clock
`(millis, counter)` that never runs behind anything observed, so every replica
picks the same winner in any arrival order. Puts are broadcast on the space's
gossip topic (topic id = space id); whenever a gossip neighbour comes up, each
side pulls the other's full state over `records-sync` (requester writes the
space id, the peer streams length-prefixed records, served only for spaces it
joined). Remote wins raise `Event::Record`. Wire:

```text
u8 1 | space[32] | u16 BE key len | key | u64 BE millis | u32 BE counter
| author[32] | u32 BE value len | value | signature[64]
signature = ed25519(author, "exp/enet/record/1\0" || every byte before it)
gossip message = 0x01 || record
```

State is in memory; persist `records(space)` and restore it with
`apply_records`.

**call**: `start_call(peer)` rings (control stream byte `0x01`); the callee
sees `Event::CallIncoming`, answers `accept_call` (`0x02`) or `hang_up`; the
caller gets `Event::CallAccepted`. `send_frame(call, bytes)` numbers frames
(`seq` from 0) and sends each as one datagram `[u32 BE seq][payload]`; a frame
larger than the path's datagram size (or a path without datagrams) goes on a
unidirectional stream `[u32 BE len][u32 BE seq][payload]` instead. Frames
arrive as `Event::CallFrame {seq, data}` in arrival order: eNet does no
encoding and no timing, the app's codec (Opus) and jitter buffer do.
Closing the connection ends the call (`Event::CallEnded {Local|Remote|Failed}`).

## Push sealing

For the push relay's accountless mode: the sender seals a small payload to
the recipient install's X25519 public key, the relay forwards opaque bytes,
the recipient's notification extension opens it (`push_open` needs no node).

```text
sealed = 0x01 || eph_pub[32] || ciphertext || tag[16]       (49 bytes overhead)
shared = X25519(eph_secret, recipient_pub)                  (all-zero refused)
okm    = HKDF-SHA256(salt = eph_pub || recipient_pub, ikm = shared,
                     info = "exp/enet/push/1", L = 44)
key = okm[0..32], nonce = okm[32..44]
ciphertext || tag = ChaCha20-Poly1305(key, nonce, aad = 0x01, plaintext)
```

A fresh ephemeral key per message makes the derived nonce single-use.
`PushKeypair::generate()` or `PushKeypair::derive(&identity)` (HKDF-SHA256,
info `exp/enet/push-x25519/1`, the 32 bytes used as the X25519 scalar,
clamped); give the extension only `secret()`.

## Security model

Every connection is QUIC with TLS 1.3, authenticated by both peers' ed25519
keys (the endpoint id IS the public key), so every channel is end-to-end
encrypted. A relay forwards encrypted QUIC packets between endpoint ids and
sees only those ids, packet sizes and timing; it never holds a key. Records
are additionally signed by their author, so a record relayed through a third
peer cannot be forged or altered. Who may connect is the app's `Access` policy.

## Why iroh (again)

VAPP-23 cancelled iroh on 2026-09-27 as "relay-only in browsers and heavy on
mobile", for a WebRTC/str0m plan that was never built. Now: iroh is 1.x
stable; the browser being relay-only is fine because the relay is the
fallback everywhere anyway and the traffic stays end-to-end encrypted; spezl
needs p2p friends, blobs and gossip, which WebRTC data channels would have
had us build ourselves; and one Rust core gives Swift/Kotlin via UniFFI and
the browser via wasm.

## Targets and features

- Native (macOS, Linux, Windows, iOS, Android): everything. iroh's default
  features are off (no portmapper, no metrics); TLS = `tls-ring`.
- `wasm32-unknown-unknown`: builds (`cargo check -p exponential-net --target
  wasm32-unknown-unknown`); relay-only, `bind_addr`/`blob_add_path`/
  `fs-store` do not exist there, blobs and gossip work (in memory).
- `fs-store`: persistent blobs (redb), native only.

## Tests

`cargo test -p exponential-net`: real nodes over localhost UDP with relays
disabled: ticket round trip, chat order + acks, refusing an unknown peer
(and the hook letting it in), blob add/fetch (by hash and ticket), records
converging across three nodes (sync on join, gossip, LWW tie-break, HLC,
bad signatures rejected), call datagrams + stream fallback + hang up, push
seal/open + tamper rejection.
