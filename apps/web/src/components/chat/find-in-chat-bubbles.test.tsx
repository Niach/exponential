// #EXP-1975 (fixtures/find-in-chat.json) ×web: EVERY bubble the chat screen
// shows is a loaded find row in drawn order (the transcript's rows, the
// synced tail, the own tail; under the no-transcript placeholder the sent
// inputs, then the own tail), and each paints its own marks.
import { cleanup, render } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { findInChatStops } from "@exp/domain-contract/find-in-chat"
import type { SessionChatRow } from "@exp/domain-contract/session-tree"
import { ChatConversation, TooltipProvider } from "@exp/ui"
import { chatScreenFindRows, widenedIds, wrapFindRow } from "@/components/agent-session"
import { renderChatText } from "@/components/chat/chat-markdown"
import { FindPaintContext, FindRow, type FindPaint } from "@/components/chat/find-in-chat"
import type { FeedItem } from "@/lib/steer-session-store"

// The genUI markdown's own marks are tested in eUI; here: which highlight
// each bubble's text receives.
vi.mock(`@/components/read-markdown`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/components/read-markdown")>()),
  ReadMarkdown: ({ text, highlight }: { text: string; highlight?: unknown }) => (
    <p data-highlight={highlight === undefined ? `` : JSON.stringify(highlight)}>{text}</p>
  ),
}))

afterEach(cleanup)

const message = (id: string, author: `user` | `agent` | `system`, body: string, extra: Partial<Record<string, unknown>> = {}) =>
  ({ type: `message`, id, author, authorId: author === `user` ? `u1` : null, body, queued: false, authorSessionId: null, xref: null, ...extra }) as SessionChatRow

const NONE = () => false
const base = { feed: [] as FeedItem[], sessionId: `s1`, isCommand: NONE, sent: [], syncedTail: [], ownTail: [], outbox: [] }

describe(`#EXP-1975 chatScreenFindRows`, () => {
  it(`a synced-tail-only chat counts the agent's and the own bubbles, then the own tail`, () => {
    const rows = chatScreenFindRows({
      ...base,
      query: `cache`,
      placeholder: false,
      syncedTail: [
        message(`a1`, `agent`, `The cache is warm, cache hit.`),
        message(`n1`, `system`, `cache note`),
        message(`u1m`, `user`, `Keep the cache.`),
        { type: `report`, id: `r1`, sessionId: `c1`, text: `Child reported`, summary: `Cache done`, xref: null } as unknown as SessionChatRow,
      ],
      ownTail: [message(`q1`, `user`, `cache behind a flag`, { queued: true })],
      outbox: [{ clientId: `c-9`, text: `one more cache` }],
    })
    expect(rows.map((row) => [row.key, row.count, row.itemId])).toEqual([
      [`m:a1`, 2, null],
      [`m:u1m`, 1, null],
      [`m:r1`, 1, null],
      [`m:q1`, 1, null],
      [`o:c-9`, 1, null],
    ])
    // The counter starts at the newest: the outbox bubble.
    const stops = findInChatStops(rows, [])
    expect(stops).toHaveLength(6)
    expect(stops[stops.length - 1]!.key).toBe(`o:c-9`)
  })

  it(`the feed's rows come first, then the synced tail`, () => {
    const feed = [{ id: 1, seq: 10, kind: `narration`, text: `cache` }] as unknown as FeedItem[]
    const rows = chatScreenFindRows({ ...base, query: `cache`, placeholder: false, feed, syncedTail: [message(`a1`, `agent`, `cache`)] })
    expect(rows.map((row) => row.key)).toEqual([`e:s1:10`, `m:a1`])
  })

  it(`under the placeholder: the sent inputs above it, then the own tail (the feed draws nothing)`, () => {
    const rows = chatScreenFindRows({
      ...base,
      query: `flag`,
      placeholder: true,
      feed: [{ id: 1, seq: 10, kind: `narration`, text: `flag` }] as unknown as FeedItem[],
      sent: [message(`s1m`, `user`, `Ship it behind a flag`), message(`s2m`, `user`, `no match`)],
      ownTail: [message(`q1`, `user`, `flag flag`, { queued: true })],
      outbox: [{ clientId: `c-1`, text: `the flag` }],
    })
    expect(rows.map((row) => [row.key, row.count])).toEqual([
      [`m:s1m`, 1],
      [`m:s2m`, 0],
      [`m:q1`, 2],
      [`o:c-1`, 1],
    ])
  })
})

describe(`#EXP-1975 the bubbles paint their marks`, () => {
  const paint = (current: FindPaint[`current`]): FindPaint => ({
    sessionId: `s1`,
    query: `cache`,
    current,
    openGroups: new Set(),
    outputHits: new Map(),
  })
  const rows = [message(`a1`, `agent`, `The cache is warm`), message(`u1m`, `user`, `Keep the cache`), message(`n1`, `system`, `cache note`)]
  const draw = (value: FindPaint | null) =>
    render(
      <TooltipProvider>
        <FindPaintContext.Provider value={value}>
          <ChatConversation rows={rows} renderText={renderChatText} attachmentSrc={(id) => id} viewerId="u1" wrapRow={wrapFindRow} />
          <FindRow findKey="o:c-1">{renderChatText(`cache in the outbox`)}</FindRow>
        </FindPaintContext.Provider>
      </TooltipProvider>
    )
  const highlightOf = (container: HTMLElement, text: string) =>
    [...container.querySelectorAll<HTMLElement>(`[data-highlight]`)].find((el) => el.textContent === text)?.dataset.highlight

  it(`the agent's and the own bubble (and an outbox bubble) get the highlight; the current stop's bubble its index`, () => {
    const { container } = draw(paint({ key: `m:u1m`, local: 0 }))
    expect(highlightOf(container, `The cache is warm`)).toBe(JSON.stringify({ query: `cache` }))
    expect(highlightOf(container, `Keep the cache`)).toBe(JSON.stringify({ query: `cache`, current: 0 }))
    expect(highlightOf(container, `cache in the outbox`)).toBe(JSON.stringify({ query: `cache` }))
    // Each bubble carries its key for landing (scroll to centre).
    const keys = [...container.querySelectorAll(`[data-find-key]`)].map((el) => el.getAttribute(`data-find-key`))
    expect(keys).toEqual([`m:a1`, `m:u1m`, `o:c-1`])
  })

  it(`no query = no marks`, () => {
    const { container } = draw(null)
    expect(highlightOf(container, `Keep the cache`)).toBe(``)
  })
})

// #EXP-1998: landing on an older synced message widens the drawn messages.
describe(`widenedIds`, () => {
  const all = [`m1`, `m2`, `m3`, `m4`, `m5`]
  it(`draws the newest max`, () => expect([...widenedIds(all, 2, null)]).toEqual([`m4`, `m5`]))
  it(`widens back to an older message`, () => expect([...widenedIds(all, 2, `m2`)]).toEqual([`m2`, `m3`, `m4`, `m5`]))
  it(`a shown or unknown one keeps the cap`, () => {
    expect([...widenedIds(all, 2, `m5`)]).toEqual([`m4`, `m5`])
    expect([...widenedIds(all, 2, `x`)]).toEqual([`m4`, `m5`])
  })
})
