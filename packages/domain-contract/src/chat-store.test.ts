// The fixture is the contract; the Rust crate `chat-store` replays it too.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/chat-store.json" with { type: "json" }
import {
  eventDocs,
  ingest,
  messageDocs,
  retain,
  searchAll,
  searchChat,
  type ChatStoreState,
  type StoredEvent,
  type StoredMessage,
} from "./chat-store"

const state = fixture.search.state as unknown as ChatStoreState

describe(`chat store docs`, () => {
  for (const entry of fixture.docs.cases) {
    test(entry.name, () => expect(eventDocs(entry.event as StoredEvent)).toEqual(entry.expect as never))
  }
  for (const entry of fixture.docs.messageCases) {
    test(entry.name, () => expect(messageDocs(entry.message as StoredMessage)).toEqual(entry.expect as never))
  }
})

describe(`chat store ingest`, () => {
  for (const entry of fixture.ingest.cases) {
    test(entry.name, () => {
      let store: ChatStoreState = { events: [], messages: [] }
      for (const batch of entry.batches) store = ingest(store, batch as never)
      expect({
        events: store.events.map((e) => ({ sessionId: e.sessionId, seq: e.seq, text: (e.event.text as string | undefined) ?? null })),
        messages: store.messages.map((m) => ({ id: m.id, body: m.body })),
      }).toEqual(entry.expect)
    })
  }
})

describe(`chat store search`, () => {
  for (const entry of fixture.search.chatCases) {
    test(entry.name, () => expect(searchChat(state, entry.rootId, entry.query, entry.limit)).toEqual(entry.expect as never))
  }
  for (const entry of fixture.search.allCases) {
    test(entry.name, () => expect(searchAll(state, entry.query, entry.limit)).toEqual(entry.expect as never))
  }
})

describe(`chat store retention`, () => {
  for (const entry of fixture.retention.cases) {
    test(entry.name, () =>
      expect(retain(fixture.retention.events as StoredEvent[], entry.now, entry.windowDays).map((e) => e.seq)).toEqual(entry.expect)
    )
  }
})
