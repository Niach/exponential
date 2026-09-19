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
    expect(container.querySelectorAll(`.bg-yellow-400`).length).toBe(1)
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
})
