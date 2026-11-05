---
title: P2P
description: Peer-to-peer, end-to-end encrypted tunnels between every Exponential client (desktop, phone, server and browser), built on iroh.
---

P2P is peer-to-peer, end-to-end encrypted networking between every Exponential client: desktop, phone, server and browser. One Rust core on [iroh](https://www.iroh.computer/), with Swift, Kotlin and WebAssembly facades. MIT or Apache-2.0.

## Status

The Rust core is published (`enet-core` 0.1.0, tag `enet-v0.1.0`) and runs on iroh 1.x with its Swift, Kotlin and WebAssembly facades. Every Exponential install runs a P2P node: the desktop app, the CLI daemon, iOS, Android and the browser. The bindings are not published on their own yet.

| Package | Registry | What | State |
|---|---|---|---|
| `enet-core` | crates.io | The Rust core | 0.1.0 |
| `@exponential_app/enet` | npm | The WebAssembly build, relay only | planned |
| `at.exponential:enet` | Maven Central | Android, through UniFFI | planned |
| `ExponentialNet` | SwiftPM | iOS and macOS, through UniFFI | planned |

The binding crates are `exponential-net-ffi` (UniFFI) and `exponential-net-wasm` (wasm-bindgen) in the Exponential repo.

## In Exponential

Session content is end-to-end encrypted. Each session tree has its own key, wrapped for every install allowed to read it; messages, cards, titles and previews are encrypted on the device that writes them, and the server stores and syncs only ciphertext plus the metadata it needs for routing and wake-ups.

Watching and steering a coding session goes over P2P's chat channel, straight from your phone or browser to the device running it. The steer relay carries no session content: it keeps device presence, remote control and voice calls. Agents on different devices message each other over P2P too, and pushes are encrypted to the phone that opens them.

Keys for a new install of yours arrive automatically from any online device that holds them. That protects against a database leak and a passive server; someone who controls the server and signs in as you can still get them. MCP clients without a device gateway, like a hosted chat app, see metadata only.

## How it connects

Every device has a key, peers dial each other by that key, and QUIC connections punch through NATs where they can. When they can't, traffic falls back to an iroh relay that we host (`https://relay.exponential.at` by default); it sees only encrypted packets. P2P uses no WebRTC.

## Channels

| Channel | What | State |
|---|---|---|
| **Chat** | Reliable, ordered, acknowledged messages between devices: steering, encrypted delivery, agent to agent | in the app |
| **Files** | Content-addressed transfers (iroh-blobs), verified on arrival | built |
| **Records** | Signed records synced between peers over gossip, last writer wins | built |
| **Calls** | Audio frames over QUIC datagrams, for calling your agent with [Voice](/docs/sdk/voice/) | in the app |
| **Push** | Notifications encrypted to the receiving device; the push relay forwards bytes it can't read | in the app |

## In the browser

A web page can't open raw UDP, so the browser build (WebAssembly) always goes through the relay. It stays end-to-end encrypted: the relay forwards bytes it can't read.

Next: [Guides](/docs/sdk/p2p/guides/) · [Reference](/docs/sdk/p2p/reference/)
