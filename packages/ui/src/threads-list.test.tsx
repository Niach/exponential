import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/threads-list.json"
import type { ThreadsListGroup } from "@exp/domain-contract/session-tree"
import { ThreadsList, threadLeadPrState, type ThreadsListPr } from "./threads-list"

// Release train 2026-10-23 (NEW-41): the row lead's PR glyph replays the
// fixture's `leadCases`, like desktop, iOS and Android.
describe(`threadLeadPrState (threads-list.json leadCases)`, () => {
  for (const entry of fixture.leadCases) {
    it(entry.name, () => {
      const prs: ThreadsListPr[] = entry.prStates.map((state, index) => ({ id: `pr${index}`, state, basePullRequestId: null }))
      expect(threadLeadPrState(prs)).toBe(entry.expected)
    })
  }
})

// S-3: a tree's hidden sub-sessions fold into ONE `<root>:earlier` row: the
// muted list item, opening the thread.
describe(`ThreadsList earlier fold`, () => {
  it(`paints the fold row muted and opens the root`, () => {
    const entry = fixture.cases.find((c) => c.name.startsWith(`S-3: one hidden child`))!
    const groups = entry.expected as unknown as ThreadsListGroup[]
    const fold = groups.flatMap((g) => g.rows).find((row) => row.earlier !== undefined)!
    const onOpenThread = vi.fn()
    const onOpenSession = vi.fn()
    render(
      <ThreadsList
        groups={groups}
        cliByRoot={new Map()}
        prsByRoot={new Map()}
        teamPrs={[]}
        onOpenThread={onOpenThread}
        onOpenSession={onOpenSession}
      />
    )
    const row = screen.getByTestId(`threads-row-earlier`)
    expect(row.textContent).toContain(fold.title)
    expect(row.className).toContain(`text-muted-foreground`)
    fireEvent.click(row)
    expect(onOpenThread).toHaveBeenCalledWith(fold.rootId)
    expect(onOpenSession).not.toHaveBeenCalled()
  })
})
