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

  it(`bolds unread rows`, () => {
    renderList(groups)
    const rows = groups.flatMap((group) => group.rows)
    const unread = rows.filter((row) => row.unread && row.title)
    for (const row of unread) {
      expect(screen.getAllByText(row.title).some((el) => el.dataset.unread === `true`)).toBe(true)
    }
  })
})

// EXP-1339: a merged thread keeps the violet lead but shows no trailing
// word, and no row ever carries the private lock.
describe(`ThreadsListView EXP-1339`, () => {
  it(`drops the merged word and the lock`, () => {
    const groups: ThreadsListGroup[] = [
      {
        key: `recent`,
        title: `Recent`,
        rows: [
          {
            sessionId: `m`,
            teamId: `t`,
            depth: 0,
            parentId: null,
            rootId: `m`,
            title: `Merged thread`,
            deviceId: null,
            busy: false,
            needsYou: false,
            unread: false,
            caption: null,
            activityAt: `2026-10-17T00:00:00.000Z`,
          } as unknown as ThreadsListGroup[`rows`][number],
        ],
      },
    ]
    render(
      <TooltipProvider>
        <ThreadsListView
          groups={groups}
          cliByRoot={new Map()}
          prsByRoot={new Map([[`m`, [{ id: `p`, state: `merged`, basePullRequestId: null }]]])}
          teamPrs={[]}
          onOpenThread={vi.fn()}
        />
      </TooltipProvider>
    )
    const row = screen.getByTestId(`threads-row`)
    expect(row.querySelector(`[data-slot="threads-row-word"]`)).toBeNull()
    expect(screen.queryByLabelText(`Private`)).toBeNull()
    expect(row.querySelector(`.text-violet-400`)).not.toBeNull()
  })
})

// EXP-1344: a root row's lead follows `threadRowMarkState`, needs-you beats
// busy: a busy row that also needs you shows the amber needs_input mark.
describe(`ThreadsListView EXP-1344`, () => {
  function leadOf(row: { busy: boolean; needsYou: boolean }) {
    const groups: ThreadsListGroup[] = [
      {
        key: `needs_you`,
        title: `Needs you`,
        rows: [
          {
            sessionId: `r`,
            teamId: `t`,
            depth: 0,
            parentId: null,
            rootId: `r`,
            title: `Busy and waiting`,
            deviceId: null,
            unread: false,
            caption: null,
            activityAt: `2026-10-17T00:00:00.000Z`,
            ...row,
          } as unknown as ThreadsListGroup[`rows`][number],
        ],
      },
    ]
    render(
      <TooltipProvider>
        <ThreadsListView
          groups={groups}
          cliByRoot={new Map([[`r`, `claude`]])}
          prsByRoot={new Map()}
          teamPrs={[]}
          onOpenThread={vi.fn()}
        />
      </TooltipProvider>
    )
    return screen.getByTestId(`threads-row`).querySelector(`[data-slot="run-mark"]`)
  }

  it(`a busy row that needs you leads with the needs_input mark`, () => {
    expect(leadOf({ busy: true, needsYou: true })?.getAttribute(`data-state`)).toBe(`needs_input`)
  })

  it(`a busy row alone leads with the working mark, not needs_input`, () => {
    expect(leadOf({ busy: true, needsYou: false })?.getAttribute(`data-state`) ?? null).not.toBe(`needs_input`)
  })
})
