import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import type { CodingSession, Device, Issue } from "@/db/schema"

// EXP-923: the sidebar's RUNNING section. Its rules: hidden with nothing
// live, one line per run (the AGENT's brand mark, an amber badge while the
// run wants you, identifier + title, the HOST DEVICE's icon at the trailing
// edge), nested by `parent_session_id`, and a click that navigates with the
// `running` origin — which is what keeps a live run off the work-tab strip.

const live = vi.hoisted(() => ({ value: [] as unknown[] }))
const devices = vi.hoisted(() => ({ value: [] as unknown[] }))
const openSession = vi.hoisted(() => vi.fn())
const location = vi.hoisted(() => ({ pathname: `/t/acme/inbox` }))

vi.mock(`@tanstack/react-router`, () => ({
  useParams: () => ({}),
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location }),
}))
vi.mock(`@tanstack/react-db`, () => ({ useLiveQuery: () => ({ data: devices.value }) }))
vi.mock(`@/lib/collections`, () => ({ deviceCollection: {} }))
vi.mock(`@/hooks/use-my-live-runs`, () => ({
  useMyLiveRuns: () => ({ runs: live.value, isReady: true, now: new Date() }),
}))
vi.mock(`@/hooks/use-agents-data`, () => ({
  rowPrState: () => null,
  useSessionListRows: (_teamId: string, sessions: CodingSession[]) =>
    sessions.map((session) => ({
      session,
      issue: issuesById.get(session.issueId ?? ``),
      batchIssues: [],
      board: undefined,
      device: { label: `Workshop`, online: true },
      paused: false,
      mergeTarget: undefined,
    })),
}))
vi.mock(`@/hooks/use-open-session`, () => ({ useOpenSession: () => openSession }))

import { SidebarRunningSection } from "@/components/team/sidebar-running"

const issuesById = new Map<string, Issue>([
  [`i1`, { id: `i1`, identifier: `EXP-923`, title: `Runs to the sidebar` } as Issue],
])

const run = (over: Partial<CodingSession>): CodingSession =>
  ({
    id: `s1`,
    issueId: `i1`,
    agent: `claude`,
    status: `running`,
    needsInput: false,
    agentBusy: false,
    parentSessionId: null,
    startedAt: new Date(),
    deviceId: `d1`,
    deviceLabel: `Workshop`,
    userId: `u1`,
    prState: null,
    ...over,
  }) as CodingSession

function renderSection() {
  return render(
    <TooltipProvider>
      <SidebarRunningSection teamId="t1" currentUserId="u1" />
    </TooltipProvider>
  )
}

describe(`SidebarRunningSection (EXP-923)`, () => {
  beforeEach(() => {
    live.value = []
    devices.value = [{ deviceId: `d1`, userId: `u1`, label: `Workshop`, kind: `server`, icon: null } as Device]
    openSession.mockClear()
    location.pathname = `/t/acme/inbox`
  })

  it(`renders nothing at all with no live run`, () => {
    const { container } = renderSection()
    expect(container.firstChild).toBeNull()
  })

  it(`lists a live run on one line and opens it with the running origin`, () => {
    live.value = [run({})]
    renderSection()
    const row = screen.getByTestId(`sidebar-running-s1`)
    expect(row.textContent).toContain(`EXP-923`)
    expect(row.textContent).toContain(`Runs to the sidebar`)
    // No status line, no "started ago" — the sidebar says what and where.
    expect(row.textContent).not.toContain(`started`)
    expect(row.textContent).not.toContain(`Workshop`)
    fireEvent.click(row)
    expect(openSession).toHaveBeenCalledTimes(1)
    expect(openSession.mock.calls[0]![1]).toEqual({
      origin: { kind: `running` },
    })
  })

  it(`badges a run that needs input, and only that one`, () => {
    live.value = [run({}), run({ id: `s2`, issueId: null, needsInput: true })]
    const { container } = renderSection()
    expect(container.querySelectorAll(`.bg-amber-500`).length).toBe(1)
  })

  // EXP-1184: the ×4 run states on the mark — Claude's spark while it works,
  // emerald with its PR open, sky once idle with no PR.
  it(`marks each run with what it is doing`, () => {
    live.value = [
      run({ agentBusy: true }),
      run({ id: `s2`, issueId: null, status: `in_review`, prState: `open` }),
      run({ id: `s3`, issueId: null }),
    ]
    renderSection()
    const mark = (id: string) =>
      screen.getByTestId(`sidebar-running-${id}`).querySelector(`[data-state]`)
    expect(mark(`s1`)?.getAttribute(`data-state`)).toBe(`working`)
    expect(
      mark(`s1`)?.querySelector(`[data-slot="claude-spinner"]`)
    ).not.toBeNull()
    expect(mark(`s2`)?.getAttribute(`data-state`)).toBe(`review`)
    expect(mark(`s2`)?.querySelector(`.bg-emerald-500`)).not.toBeNull()
    expect(mark(`s3`)?.getAttribute(`data-state`)).toBe(`done`)
    expect(mark(`s3`)?.querySelector(`.bg-sky-500`)).not.toBeNull()
  })

  it(`nests a child run under its parent and draws the connector`, () => {
    live.value = [run({}), run({ id: `s2`, issueId: null, parentSessionId: `s1` })]
    const { container } = renderSection()
    const rows = container.querySelectorAll(`[data-testid^="sidebar-running-"]`)
    expect(rows.length).toBe(2)
    expect((rows[1] as HTMLElement).style.paddingLeft).toBe(`26px`)
    // EXP-965: only the CHILD carries guides.
    expect(container.querySelectorAll(`[data-testid="tree-guides"]`).length).toBe(1)
  })

  it(`highlights the run the app is showing, on either face`, () => {
    live.value = [run({})]
    location.pathname = `/t/acme/sessions/s1`
    const { unmount } = renderSection()
    expect(
      screen.getByTestId(`sidebar-running-s1`).className
    ).toContain(`bg-glass-active`)
    unmount()
    location.pathname = `/t/acme/inbox`
    renderSection()
    expect(
      screen.getByTestId(`sidebar-running-s1`).className
    ).not.toContain(`bg-glass-active`)
  })

  // EXP-1248: no fold chevron anywhere: a parent's mark is the row's first
  // glyph at the base inset, exactly like a standalone root's.
  it(`leads every row with the mark, a parent included, and draws no chevron`, () => {
    live.value = [
      run({}),
      run({ id: `s2`, issueId: null, parentSessionId: `s1` }),
      run({ id: `s3`, issueId: null }),
    ]
    const { container } = renderSection()
    expect(container.querySelector(`[role="button"][aria-label*="child runs"]`)).toBeNull()
    for (const id of [`s1`, `s2`, `s3`]) {
      const row = screen.getByTestId(`sidebar-running-${id}`)
      const first = Array.from(row.children).find(
        (child) => child.getAttribute(`data-testid`) !== `tree-guides`
      )
      expect(first?.getAttribute(`data-slot`)).toBe(`run-mark`)
      expect(row.getAttribute(`data-session-row`)).toBe(`small`)
    }
    expect(screen.getByTestId(`sidebar-running-s1`).style.paddingLeft).toBe(`12px`)
    expect(screen.getByTestId(`sidebar-running-s3`).style.paddingLeft).toBe(`12px`)
  })
})
