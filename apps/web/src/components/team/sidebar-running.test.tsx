import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SidebarProvider, TooltipProvider } from "@exp/ui"
import type { ThreadsListGroup } from "@/lib/session-tree"

// EXP-923 / EXP-1293 / EXP-1318: the sidebar's ACTIVE section lists the
// Threads list's `needs_you` + `running` rows, then the Recent rows whose
// word still asks for a merge (`railThreads`) — hidden when empty, one row
// per thread (the agent mark and the title, NO identifier, NO caption), and
// a click that opens the THREAD with the `running` origin — which keeps it
// off the work-tab strip.

const navigate = vi.hoisted(() => vi.fn())
const location = vi.hoisted(() => ({ pathname: `/t/acme/inbox` }))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => navigate,
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location }),
}))
vi.mock(`@/lib/collections`, () => ({
  sessionCollection: {
    get: (id: string) =>
      id === `s-child` ? { id, rootId: `t-run`, cli: `claude` } : { id, rootId: id, cli: `claude` },
  },
}))

vi.mock(`@/hooks/use-device-glyphs`, () => ({
  useDeviceGlyphs: () => (id: string | null) =>
    id === `d-mac` ? { icon: () => <svg data-testid="device-glyph" />, name: `Alex's MacBook Pro` } : null,
}))

import {
  SidebarRunningIcons,
  SidebarRunningSection,
  threadRunMarkState,
  threadsNavState,
} from "@/components/team/sidebar-running"

const T0 = `2026-10-13T10:00:00.000Z`

function row(
  sessionId: string,
  title: string,
  over: { busy?: boolean; needsYou?: boolean; deviceId?: string | null } = {}
) {
  return {
    sessionId,
    teamId: `team-1`,
    title,
    private: false,
    deviceId: over.deviceId ?? null,
    busy: over.busy ?? false,
    needsYou: over.needsYou ?? false,
    activityAt: T0,
  }
}

const groups: ThreadsListGroup[] = [
  {
    key: `needs_you`,
    title: `Needs you`,
    rows: [row(`t-ask`, `Reduce cold start`, { needsYou: true, deviceId: `d-mac` })],
  },
  { key: `running`, title: `Running`, rows: [row(`t-run`, `Group board issues`, { busy: true })] },
  {
    key: `recent`,
    title: `Recent`,
    rows: [row(`t-old`, `Fix flaky scroll`), row(`t-pr`, `Implement APP-9`)],
  },
]
const wordOf = (r: { sessionId: string }) => (r.sessionId === `t-pr` ? (`readyToMerge` as const) : null)

function renderSection(list: ThreadsListGroup[] = groups) {
  return render(
    <SidebarProvider>
      <TooltipProvider>
        <SidebarRunningSection teamSlug="acme" threads={{ groups: list, wordOf }} />
      </TooltipProvider>
    </SidebarProvider>
  )
}

beforeEach(() => {
  navigate.mockReset()
  location.pathname = `/t/acme/inbox`
})

describe(`SidebarRunningSection`, () => {
  it(`is hidden with nothing active`, () => {
    renderSection([{ ...groups[2], rows: [groups[2].rows[0]] }])
    expect(screen.queryByTestId(`sidebar-running`)).toBeNull()
  })

  it(`is the Active band: needs-you, running, then recent threads waiting on a merge`, () => {
    renderSection()
    expect(screen.getByText(`Active`)).toBeTruthy()
    const rows = screen.getAllByTestId(/^sidebar-running-/)
    expect(rows.map((el) => el.getAttribute(`data-testid`))).toEqual([
      `sidebar-running-t-ask`,
      `sidebar-running-t-run`,
      `sidebar-running-t-pr`,
    ])
    expect(rows[0].textContent).toBe(`Reduce cold start`)
    // EXP-1318: the root's device glyph trails the row (named for AT).
    expect(
      rows[0].querySelector(`[data-slot="session-row-device"]`)?.getAttribute(`aria-label`)
    ).toBe(`Alex's MacBook Pro`)
    expect(rows[1].querySelector(`[data-slot="session-row-device"]`)).toBeNull()
    expect(screen.queryByText(`Fix flaky scroll`)).toBeNull()
    // The needs-you thread wears the amber badge.
    expect(rows[0].querySelector(`[data-slot="run-mark"]`)?.getAttribute(`data-state`)).toBe(
      `needs_input`
    )
  })

  it(`opens the thread with the tabless running origin`, () => {
    renderSection()
    fireEvent.click(screen.getByTestId(`sidebar-running-t-run`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `t-run` },
      search: { from: `running` },
    })
  })

  it(`marks the thread on screen, its own page or one of its sessions`, () => {
    location.pathname = `/t/acme/sessions/s-child`
    renderSection()
    expect(screen.getByTestId(`sidebar-running-t-run`).getAttribute(`data-active`)).toBe(`true`)
    expect(screen.getByTestId(`sidebar-running-t-ask`).getAttribute(`data-active`)).toBe(`false`)
  })
})

describe(`SidebarRunningIcons`, () => {
  it(`hands the rail one item per Active thread`, () => {
    const items: { key: string; label: string }[] = []
    render(
      <SidebarRunningIcons
        teamSlug="acme"
        threads={{ groups, wordOf }}
        renderItem={(item) => {
          items.push(item)
          return null
        }}
      />
    )
    expect(items.map((item) => [item.key, item.label])).toEqual([
      [`t-ask`, `Reduce cold start`],
      [`t-run`, `Group board issues`],
      [`t-pr`, `Implement APP-9`],
    ])
  })
})

describe(`the nav state`, () => {
  it(`is green while anything runs, amber while something needs you`, () => {
    expect(threadsNavState([])).toEqual({ live: false, needsYou: false })
    expect(threadsNavState([groups[1], groups[2]])).toEqual({ live: true, needsYou: false })
    expect(threadsNavState(groups)).toEqual({ live: true, needsYou: true })
  })

  it(`maps a row onto the run mark`, () => {
    expect(threadRunMarkState({ needsYou: true, busy: true })).toBe(`needs_input`)
    expect(threadRunMarkState({ needsYou: false, busy: true })).toBe(`working`)
    expect(threadRunMarkState({ needsYou: false, busy: false })).toBeUndefined()
  })
})
