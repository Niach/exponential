// #EDE-129 (fixtures/find-in-chat.json) ×web: the transcript's countable
// rows (what the bubbles and tool lines paint, a folded run's members), the
// `…` entry (first, its own group, the shortcut hint) and the bar's keys.
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { FIND_IN_CHAT_COPY, FIND_IN_CHAT_TEST_IDS } from "@exp/domain-contract/find-in-chat"
import type { MenuEntry } from "@exp/ui"
import { transcriptFindKeys, transcriptFindRows } from "@/components/agent-session"
import { FindInChatBar, findInChatMenuEntries, type FindInChat } from "@/components/chat/find-in-chat"
import type { FeedItem } from "@/lib/steer-session-store"

afterEach(cleanup)

const feed = [
  { id: 1, seq: 10, kind: `user_message`, text: `Cold start is at 1.4 s. Cut the sync bootstrap.` },
  { id: 2, seq: 11, kind: `tool`, name: `Bash`, detail: `rg "snapshot cache" apps/android`, toolKind: `execute`, settled: true, callId: `a` },
  { id: 3, seq: 12, kind: `tool`, name: `Bash`, detail: `./gradlew :app:benchmark`, toolKind: `execute`, settled: true, callId: `b`, output: `snapshot Cache warm\nok` },
  { id: 4, seq: 13, kind: `narration`, text: `Added a board snapshot cache. The cache is warm.` },
  { id: 5, seq: 14, kind: `thought`, thoughtId: `t`, text: `cache cache`, settled: true },
  { id: 6, seq: 14, kind: `user_message`, text: `Keep the cache behind a flag please.` },
] as unknown as FeedItem[]

describe(`#EDE-129 transcriptFindRows`, () => {
  it(`counts what the rows paint, top to bottom; a run's members carry its group`, () => {
    const rows = transcriptFindRows(feed, `s1`, `cache`, () => false)
    expect(rows.map((row) => [row.key, row.count, row.group ?? null, row.outputHit ?? false])).toEqual([
      [`e:s1:10`, 0, null, false],
      [`e:s1:11`, 1, `g:2`, false],
      [`e:s1:12`, 0, `g:2`, true],
      [`e:s1:13`, 2, null, false],
      [`e:s1:14#6`, 1, null, false],
    ])
    expect(rows.find((row) => row.key === `e:s1:12`)?.outputCount).toBe(1)
  })

  it(`an echo that inherited a seq gets its own key`, () => {
    expect(transcriptFindKeys(feed, `s1`).get(6)).toBe(`e:s1:14#6`)
    expect(transcriptFindKeys(feed, `s1`).get(5)).toBe(`e:s1:14`)
  })
})

describe(`#EDE-129 the … entry`, () => {
  const del: MenuEntry = { kind: `item`, id: `delete`, label: `Delete`, onSelect: () => {} }
  it(`is first, in its own group, with the shortcut hint`, () => {
    const entries = findInChatMenuEntries([del], () => {}, false)
    expect(entries.map((entry) => entry.id)).toEqual([`search`, `sep-search`, `delete`])
    const item = entries[0] as Extract<MenuEntry, { kind: `item` }>
    expect(item.label).toBe(FIND_IN_CHAT_COPY.menu)
    expect(item.shortcut).toMatch(/^(⌘F|Ctrl F)$/)
  })
  it(`phones show no hint; no find = unchanged`, () => {
    expect((findInChatMenuEntries([], () => {}, true)[0] as { shortcut?: string }).shortcut).toBeUndefined()
    expect(findInChatMenuEntries([del], undefined, false)).toEqual([del])
  })
})

describe(`#EDE-129 the bar`, () => {
  const controller = (over: Partial<FindInChat> = {}): FindInChat => ({
    open: true,
    raw: `cache`,
    query: `cache`,
    step: null,
    report: { counter: `2 of 7`, status: `Searching older messages…` },
    openBar: vi.fn(),
    close: vi.fn(),
    setRaw: vi.fn(),
    go: vi.fn(),
    setReport: vi.fn(),
    focusNonce: 1,
    ...over,
  })
  it(`shows the counter and status; Enter = previous, Shift+Enter = next, Escape closes`, () => {
    const find = controller()
    render(<FindInChatBar find={find} />)
    expect(screen.getByTestId(FIND_IN_CHAT_TEST_IDS.counter).textContent).toBe(`2 of 7`)
    expect(screen.getByTestId(FIND_IN_CHAT_TEST_IDS.status).textContent).toBe(`Searching older messages…`)
    const input = screen.getByTestId(FIND_IN_CHAT_TEST_IDS.input)
    fireEvent.keyDown(input, { key: `Enter` })
    fireEvent.keyDown(input, { key: `Enter`, shiftKey: true })
    expect(find.go).toHaveBeenNthCalledWith(1, `previous`)
    expect(find.go).toHaveBeenNthCalledWith(2, `next`)
    fireEvent.keyDown(input, { key: `Escape` })
    expect(find.close).toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(FIND_IN_CHAT_TEST_IDS.previous))
    expect(find.go).toHaveBeenLastCalledWith(`previous`)
  })
  it(`closed = nothing`, () => {
    const { container } = render(<FindInChatBar find={controller({ open: false })} />)
    expect(container.innerHTML).toBe(``)
  })
})
