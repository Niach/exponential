import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import fixture from "@exp/domain-contract/fixtures/threads-list.json"
import { partitionThreads, type ThreadsListGroup } from "@/lib/session-tree"

// EXP-1323: the Threads list nests a live tree's children under the root
// (SessionRow depth + TreeGuides), a child opens its session, bold = unread
// and the live caption is the muted line — over the contract's own case.

vi.mock(`@/lib/collections`, () => ({
  pullRequestCollection: {},
  sessionCollection: { get: () => undefined },
}))
vi.mock(`@/hooks/use-device-glyphs`, () => ({ useDeviceGlyphs: () => () => null }))

import { ThreadsListView } from "@/components/thread/threads-list"

type Input = Parameters<typeof partitionThreads>[0]
const NESTED = fixture.cases.find((entry) => entry.name.startsWith(`EXP-1323`))!

function renderList(groups: ThreadsListGroup[]) {
  const onOpenThread = vi.fn()
  const onOpenSession = vi.fn()
  render(
    <TooltipProvider>
      <ThreadsListView
        groups={groups}
        cliByRoot={new Map()}
        prsByRoot={new Map()}
        teamPrs={[]}
        onOpenThread={onOpenThread}
        onOpenSession={onOpenSession}
      />
    </TooltipProvider>
  )
  return { onOpenThread, onOpenSession }
}

describe(`ThreadsListView EXP-1323`, () => {
  const groups = partitionThreads(NESTED.input as unknown as Input)
  const children = groups.flatMap((group) => group.rows).filter((row) => row.depth > 0)

  it(`nests every child of a live tree as a session row`, () => {
    renderList(groups)
    expect(children.length).toBeGreaterThan(0)
    expect(screen.getAllByTestId(`threads-row-child`)).toHaveLength(children.length)
  })

  it(`a child opens its session, a root its thread`, () => {
    const { onOpenThread, onOpenSession } = renderList(groups)
    const first = children[0]!
    fireEvent.click(screen.getAllByTestId(`threads-row-child`)[0]!)
    expect(onOpenSession).toHaveBeenCalledWith(first.rootId, first.sessionId)
    fireEvent.click(screen.getAllByTestId(`threads-row`)[0]!)
    expect(onOpenThread).toHaveBeenCalled()
  })

  it(`bolds unread rows and shows captions`, () => {
    renderList(groups)
    const rows = groups.flatMap((group) => group.rows)
    const unread = rows.filter((row) => row.unread && row.title)
    for (const row of unread) {
      expect(screen.getAllByText(row.title).some((el) => el.dataset.unread === `true`)).toBe(true)
    }
    for (const row of rows.filter((row) => row.caption)) {
      expect(screen.getAllByText(row.caption!).length).toBeGreaterThan(0)
    }
  })
})
