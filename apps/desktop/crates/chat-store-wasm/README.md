# `chat-store-wasm`: the web's chat store

The wasm-bindgen facade over [`chat-store`](../chat-store) (#EDE-115, phase
S4 of `packages/domain-contract/fixtures/chat-store.json`). The web keeps one
APPEND-ONLY LOG per chat (root) in OPFS: a `ChatFile` replays it on load,
every write returns the lines to append (only what changed: events dedup by
(session, device, seq), messages latest wins), and `snapshot` rewrites it
whole after RETENTION. Reads (the REPLAY TAIL a chat opens with, older
pages, find in chat) and the catch-up rules (`have`, the holder's
`history_chunk`, the asker's progress, holders, the byte bucket) are the
crate's own.

```js
import init, { ChatFile } from "/chat-store/chat_store_wasm.js"
await init()
const file = new ChatFile()
file.load(text)                                   // the log, as read
const append = file.putEvents(JSON.stringify(events))
const tail = JSON.parse(file.openTail(sessionId, deviceId, replayTail))
```

## Build

`scripts/build-chat-store-wasm.sh` (repo root) → `apps/web/public/chat-store/`
(gitignored, `--target web`); `build-web.yml` runs it before the image build
like the eNet wasm. The web loads it lazily in its chat-store worker
(`apps/web/src/lib/chat-store/`), never in the main bundle. Needs the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the `wasm-bindgen`
version in `apps/desktop/Cargo.lock`.

Licence: Apache-2.0
