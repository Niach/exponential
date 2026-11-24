// The fixture is the contract; the Rust crate `chat-store` replays it too.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/chat-store.json" with { type: "json" }
import {
  catchUpAnswer,
  catchUpBucket,
  catchUpHolders,
  catchUpMayRun,
  catchUpProgress,
  catchUpRetryMs,
  catchUpWalk,
  chatStoreOnLeave,
  eventDocs,
  haveRuns,
  ingest,
  messageDocs,
  retain,
  searchAll,
  systemInput,
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
        events: store.events.map((e) => ({ sessionId: e.sessionId, deviceId: e.deviceId, seq: e.seq, text: (e.event.text as string | undefined) ?? null })),
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
  // #FEED-300: SYSTEM INPUTS.
  for (const entry of fixture.search.systemCases) {
    test(`system input: ${entry.name}`, () => expect(systemInput(entry.text)).toBe(entry.expected))
  }
})

describe(`chat store retention`, () => {
  for (const entry of fixture.retention.cases) {
    test(entry.name, () =>
      expect(retain(fixture.retention.events as StoredEvent[], entry.now, entry.windowDays).map((e) => e.seq)).toEqual(entry.expect)
    )
  }
})

describe(`chat store catch-up (#EDE-108)`, () => {
  const c = fixture.catchUp
  for (const entry of c.haveRunsCases) {
    test(`have runs: ${entry.name}`, () => expect(haveRuns(entry.seqs, entry.afterSeq, entry.max)).toEqual(entry.expect as never))
  }
  for (const entry of c.answerCases as unknown as {
    name: string
    events?: { seq: number; kind: string; at: number }[]
    eventRange?: [number, number]
    fullTo: number | null
    ask: { afterSeq: number | null; limit: number; have: [number, number][]; minAt?: number }
    expect: unknown
  }[]) {
    const events =
      entry.events ??
      Array.from({ length: entry.eventRange![1] - entry.eventRange![0] + 1 }, (_, i) => ({ seq: entry.eventRange![0] + i, kind: `narration`, at: 0 }))
    test(`answer: ${entry.name}`, () => expect(catchUpAnswer(events, entry.fullTo, entry.ask)).toEqual(entry.expect as never))
  }
  for (const entry of c.progressCases) {
    test(`progress: ${entry.name}`, () => expect(catchUpProgress(entry.completeTo, entry.afterSeq, entry.answer)).toEqual(entry.expect as never))
  }
  for (const entry of c.walkCases) {
    test(`walk: ${entry.name}`, () => expect(catchUpWalk(entry.asker, entry.holders as never, entry.limit)).toEqual(entry.expect as never))
  }
  for (const entry of c.holderCases) {
    test(`holders: ${entry.name}`, () =>
      expect(catchUpHolders(entry.installs as never, entry.deviceId, entry.selfEndpoint, entry.userId, entry.skip)).toEqual(entry.expect))
  }
  for (const entry of c.gateCases) {
    test(`power: ${entry.name}`, () => expect(catchUpMayRun(entry.power as never, entry.idleMs)).toBe(entry.run))
  }
  for (const entry of c.retryCases) {
    test(`retry after ${entry.attempts}`, () => expect(catchUpRetryMs(entry.attempts)).toBe(entry.ms))
  }
  for (const entry of c.bucketCases) {
    test(`bucket: ${entry.name}`, () => expect(catchUpBucket(entry.ops as never)).toEqual(entry.waits))
  }
})

describe(`chat store sign-out`, () => {
  for (const entry of fixture.signOut.cases) {
    test(entry.name, () => expect(chatStoreOnLeave(entry.reason as never, entry.account, entry.files)).toEqual(entry.expect))
  }
})
