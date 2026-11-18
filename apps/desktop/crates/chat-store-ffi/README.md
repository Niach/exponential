# `chat-store-ffi`: the phones' chat store

#EDE-114, phase S3 of `packages/domain-contract/fixtures/chat-store.json`. The
`chat-store` crate's SQLite adapter behind UniFFI (namespace `chat_store_ffi`,
Swift module `ExponentialChatStoreFFI`, Kotlin `at.exponential.chatstore.ffi`),
linked into the apps' one library, [`exponential-mobile-ffi`](../exponential-mobile-ffi).

* `ChatStore`: one account's store, `{dataDir}/accounts/<account>/chat-store.sqlite`
  under the app's protected directory; `putFeed`, `putMessages`, `openTail`
  (OPEN through the REPLAY TAIL), `page`, `searchChat`/`searchAll`,
  `applyRetention`, `answerPage` (the holder's `history_chunk`).
* `CatchUp`: the `catchUp` walk without I/O: `beginPass` → `nextWalk` →
  `nextAsk` / `takeAnswer` until `nextAsk` is null; the app carries the frames
  over eNet and decides when its OS lets a pass run.
* `deleteAccountStore`: a deliberate sign-out (#EDE-120).

Bindings are COMMITTED (`bash generate-bindings.sh` regenerates them). Tests:
`cargo test -p chat-store-ffi`; the open time on a 17k-event chat:
`cargo test -p chat-store-ffi --release open_time -- --ignored --nocapture`.
