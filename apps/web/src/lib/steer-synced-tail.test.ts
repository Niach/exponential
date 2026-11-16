import { describe, expect, it } from "vitest"
import { steerOfflineLine } from "@exp/domain-contract/steer-enet"
import { syncedTailFeedRows, syncedTailMessages } from "@/lib/steer-synced-tail"
import type { FeedItem } from "@/lib/steer-session-store"

const ms = (iso: string) => Date.parse(iso)

// #FEED-260: the request, `Ran 3 commands` (one settled), the device offline.
const feed: FeedItem[] = [
  { id: 1, seq: 1, kind: `user_message`, text: `Can I build a genUI renderer for WoW addons?`, at: ms(`2026-11-16T14:32:29.000Z`) },
  { id: 2, seq: 2, kind: `tool`, name: `Bash`, settled: true, settledAt: ms(`2026-11-16T14:33:10.000Z`) },
  { id: 3, seq: 3, kind: `tool`, name: `Bash` },
  // A subagent's prose is not the main lane: it never hides the answer.
  {
    id: 4,
    seq: 4,
    kind: `narration`,
    text: `**Short answer:** yes`,
    subagentId: `sub-1`,
    at: ms(`2026-11-16T14:33:20.000Z`),
  },
]

const messages = [
  {
    id: `u1`,
    authorKind: `user`,
    card: null,
    consumedAt: `2026-11-16T14:32:29.000Z`,
    createdAt: `2026-11-16T14:32:28.000Z`,
    body: `Can I build a genUI renderer for WoW addons?`,
  },
  {
    id: `a1`,
    authorKind: `agent`,
    card: null,
    consumedAt: null,
    createdAt: `2026-11-16T14:33:58.000Z`,
    body: `**Short answer:** yes, you can build a genUI renderer as a WoW addon library.`,
  },
]

describe(`#FEED-260 synced tail`, () => {
  it(`maps the main lane: inputs, settled tools, narrations`, () => {
    expect(syncedTailFeedRows(feed)).toEqual([
      { author: `user`, text: `Can I build a genUI renderer for WoW addons?`, at: `2026-11-16T14:32:29.000Z` },
      { author: null, text: null, at: `2026-11-16T14:33:10.000Z` },
      { author: null, text: null, at: null },
    ])
  })

  it(`a not-live feed ending in a tool run draws the synced answer, then the offline line`, () => {
    const tail = syncedTailMessages({ live: false, feed, messages })
    expect(tail.map((m) => m.id)).toEqual([`a1`])
    expect(
      steerOfflineLine({ live: false, rows: feed.length + tail.length, ended: false, noFeed: false, deviceOffline: true })
    ).toBe(true)
  })

  it(`a live feed is the authority`, () => {
    expect(syncedTailMessages({ live: true, feed, messages })).toEqual([])
  })

  it(`a narration the feed holds is not drawn twice`, () => {
    const held: FeedItem[] = [
      ...feed,
      {
        id: 5,
        seq: 5,
        kind: `narration`,
        text: `**Short answer:** yes, you can build a genUI renderer as a WoW addon library.\n`,
        at: ms(`2026-11-16T14:33:20.000Z`),
      },
    ]
    expect(syncedTailMessages({ live: false, feed: held, messages })).toEqual([])
  })

  it(`an empty feed of an unreachable device takes the tail with its agent line`, () => {
    expect(syncedTailMessages({ live: false, feed: [], messages, unreachable: true }).map((m) => m.id)).toEqual([`u1`, `a1`])
  })

  it(`an empty feed of a reachable device keeps its placeholder`, () => {
    expect(syncedTailMessages({ live: false, feed: [], messages })).toEqual([])
  })
})
