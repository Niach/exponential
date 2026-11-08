// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/dock.json" with { type: "json" }
import toastFixture from "../fixtures/toast-stack.json" with { type: "json" }
import { dock, dockAutoOpen, dockRows, suggestionSources, chatDockQueue, chatQueueChildState, chatQueueProbe, DOCK_AUTO_OPEN_GRACE_MS, DOCK_STACK, type DockKind, type DockRow, type DockSource, type ChatQueueCandidate, type ChatQueueChildState, type ChatQueueProbe } from "./dock"

describe(`dockRows`, () => {
  for (const entry of fixture.rowCases) {
    test(entry.name, () => {
      expect(dockRows(entry.sources as DockSource[])).toEqual(entry.expected as DockRow[])
    })
  }
})

describe(`dock`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(dock(entry.input as { rows: DockRow[]; expanded: boolean; openId: string | null })).toEqual(
        entry.expected as never
      )
    })
  }
})

describe(`suggestionSources`, () => {
  for (const entry of fixture.suggestionCases) {
    test(entry.name, () => {
      expect(suggestionSources(entry.rows as never)).toEqual(entry.expected as never)
    })
  }
})

describe(`dockAutoOpen`, () => {
  for (const entry of fixture.autoOpenCases) {
    test(entry.name, () => {
      const rows = entry.rowIds.map(([id, kind]) => ({ id: id!, kind: kind as DockKind }))
      expect(dockAutoOpen({ previous: entry.previous, rows })).toBe(entry.expected)
    })
  }
})

describe(`chatDockQueue`, () => {
  for (const entry of fixture.chatQueueCases) {
    test(entry.name, () => {
      expect(chatDockQueue(entry.candidates as ChatQueueCandidate[])).toEqual(entry.expected)
    })
  }
})

describe(`chatQueueProbe (#EXP-1678: one child stream)`, () => {
  for (const entry of fixture.chatQueueProbeCases) {
    test(entry.name, () => {
      expect(chatQueueProbe(entry.rootOpen, entry.children as ChatQueueProbe[])).toBe(entry.expected)
    })
  }
  for (const entry of fixture.childStateCases) {
    test(`child state: ${entry.name}`, () => {
      expect(chatQueueChildState(entry as Parameters<typeof chatQueueChildState>[0])).toBe(entry.state as ChatQueueChildState)
    })
  }
})

test(`the auto-open grace is on record`, () => {
  expect(DOCK_AUTO_OPEN_GRACE_MS).toBe(1500)
})

test(`the pile is the toast's stack (peek, scale step, visible)`, () => {
  expect(DOCK_STACK.peek).toBe(toastFixture.constants.peek)
  expect(DOCK_STACK.scaleStep).toBe(toastFixture.constants.scaleStep)
  expect(DOCK_STACK.visible).toBe(toastFixture.constants.visible)
})
