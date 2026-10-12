import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import type { CodingSession, Issue, Thread } from "@/db/schema"
import { buildThreadView } from "@/lib/thread-view"
import type { ThreadBundle } from "@/hooks/use-threads"

// EXP-923 / EXP-1284: the sidebar's RUNNING section lists THREADS. Its
// rules: hidden with nothing live of mine, one row per thread (its title
// over the model's list caption, the live run's mark leading, an amber
// badge while that run waits on a person), and a click that opens the
// THREAD page with the `running` origin — which keeps it off the work-tab
// strip.

const running = vi.hoisted(() => ({ value: [] as unknown[] }))
const navigate = vi.hoisted(() => vi.fn())
const location = vi.hoisted(() => ({ pathname: `/t/acme/inbox` }))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => navigate,
  useParams: () => ({ teamSlug: `acme` }),
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location }),
}))
vi.mock(`@/hooks/use-threads`, () => ({
  useThreadList: () => ({ running: running.value, recent: [], isReady: true, now: new Date() }),
}))

import { SidebarRunningSection } from "@/components/team/sidebar-running"

const T0 = `2026-10-12T10:00:00.000Z`

function bundle(over: {
  id: string
  title?: string
  needsInput?: boolean
  agentBusy?: boolean
}): ThreadBundle {
  const run = {
    id: `r-${over.id}`,
    agent: `claude`,
    userId: `u1`,
    status: `running`,
    needsInput: over.needsInput ?? false,
    agentBusy: over.agentBusy ?? false,
    issueId: null,
    parentSessionId: null,
    resumedFromId: null,
    startedAt: T0,
    endedAt: null,
    deviceLabel: `macbook`,
  } as unknown as CodingSession
  const issue = { id: `i-${over.id}`, identifier: `EXP-1`, title: `An issue`, createdAt: T0 } as unknown as Issue
  const view = buildThreadView({
    thread: { id: over.id, title: over.title ?? null, results: null, updatedAt: T0 },
    items: [
      { id: `a`, kind: `run`, issueId: null, sessionId: run.id, source: `user`, addedBySessionId: null, sortOrder: 1, createdAt: T0 },
      { id: `b`, kind: `issue`, issueId: issue.id, sessionId: null, source: `user`, addedBySessionId: null, sortOrder: 2, createdAt: T0 },
    ],
    issues: [{ id: issue.id, identifier: `EXP-1`, title: `An issue`, createdAt: T0 }],
    runs: [
      {
        id: run.id,
        name: over.title ?? `Chat`,
        userId: `u1`,
        deviceLabel: `macbook`,
        status: `running`,
        needsInput: over.needsInput ?? false,
        agentBusy: over.agentBusy ?? false,
        issueId: null,
        parentSessionId: null,
        resumedFromId: null,
        startedAt: T0,
        endedAt: null,
      },
    ],
  })
  return {
    thread: { id: over.id, teamId: `t1`, title: over.title ?? null, results: null } as unknown as Thread,
    items: [],
    runs: [run],
    issues: [issue],
    view,
    runNames: new Map([[run.id, over.title ?? `Chat`]]),
    mineLive: true,
    myLiveRun: run,
    liveRun: run,
  }
}

function renderSection() {
  return render(
    <TooltipProvider>
      <SidebarRunningSection teamId="t1" currentUserId="u1" />
    </TooltipProvider>
  )
}

describe(`SidebarRunningSection (EXP-923, EXP-1284 threads)`, () => {
  beforeEach(() => {
    running.value = []
    navigate.mockClear()
    location.pathname = `/t/acme/inbox`
  })

  it(`renders nothing at all with no running thread`, () => {
    const { container } = renderSection()
    expect(container.firstChild).toBeNull()
  })

  it(`lists a thread with its caption and opens its page with the running origin`, () => {
    const thread = bundle({ id: `th1`, title: `Keyboard and picker` })
    running.value = [thread]
    renderSection()
    const row = screen.getByTestId(`sidebar-running-th1`)
    expect(row.textContent).toContain(`Keyboard and picker`)
    // The model's list caption (`1 issue · 1 run · macbook`), verbatim.
    expect(thread.view.listCaption).toMatch(/^1 issue · 1 \w+ · macbook$/)
    expect(row.textContent).toContain(thread.view.listCaption)
    fireEvent.click(row)
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `th1` },
      search: { from: `running` },
    })
  })

  it(`badges a thread whose run needs input, and only that one`, () => {
    running.value = [bundle({ id: `th1` }), bundle({ id: `th2`, needsInput: true })]
    const { container } = renderSection()
    expect(container.querySelectorAll(`.bg-amber-500`).length).toBe(1)
    expect(
      screen.getByTestId(`sidebar-running-th2`).querySelector(`[data-state]`)?.getAttribute(`data-state`)
    ).toBe(`needs_input`)
  })

  it(`sparks while the thread's run works`, () => {
    running.value = [bundle({ id: `th1`, agentBusy: true })]
    renderSection()
    const mark = screen.getByTestId(`sidebar-running-th1`).querySelector(`[data-state]`)
    expect(mark?.getAttribute(`data-state`)).toBe(`working`)
  })

  it(`highlights the thread the app is showing — its page or one of its runs`, () => {
    running.value = [bundle({ id: `th1` })]
    location.pathname = `/t/acme/threads/th1`
    const first = renderSection()
    expect(screen.getByTestId(`sidebar-running-th1`).className).toContain(`bg-glass-active`)
    first.unmount()
    location.pathname = `/t/acme/sessions/r-th1`
    const second = renderSection()
    expect(screen.getByTestId(`sidebar-running-th1`).className).toContain(`bg-glass-active`)
    second.unmount()
    location.pathname = `/t/acme/inbox`
    renderSection()
    expect(screen.getByTestId(`sidebar-running-th1`).className).not.toContain(`bg-glass-active`)
  })
})
