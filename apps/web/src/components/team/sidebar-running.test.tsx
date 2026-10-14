import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SidebarProvider, TooltipProvider } from "@exp/ui"
import type { ThreadsListGroup } from "@/lib/session-tree"

// EXP-923 / EXP-1293: the sidebar's RUNNING section lists the Threads list's
// `needs_you` + `running` rows — hidden with nothing live, one row per
// thread (the agent mark, the first issue's identifier, the title, NO
// caption), and a click that opens the THREAD with the `running` origin —
// which keeps it off the work-tab strip.

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

import {
  SidebarRunningIcons,
  SidebarRunningSection,
  runningThreadRows,
  threadRunMarkState,
  threadsNavState,
} from "@/components/team/sidebar-running"

const T0 = `2026-10-13T10:00:00.000Z`

function row(sessionId: string, title: string, over: { busy?: boolean; needsYou?: boolean } = {}) {
  return {
    sessionId,
    teamId: `team-1`,
    title,
    private: false,
    busy: over.busy ?? false,
    needsYou: over.needsYou ?? false,
    activityAt: T0,
  }
}

const groups: ThreadsListGroup[] = [
  { key: `needs_you`, title: `Needs you`, rows: [row(`t-ask`, `Reduce cold start`, { needsYou: true })] },
  { key: `running`, title: `Running`, rows: [row(`t-run`, `Group board issues`, { busy: true })] },
  { key: `recent`, title: `Recent`, rows: [row(`t-old`, `Fix flaky scroll`)] },
]
const extras = { identifierByRoot: new Map([[`t-ask`, `APP-5`]]) }

function renderSection(list: ThreadsListGroup[] = groups) {
  return render(
    <SidebarProvider>
      <TooltipProvider>
        <SidebarRunningSection teamSlug="acme" threads={{ groups: list, extras }} />
      </TooltipProvider>
    </SidebarProvider>
  )
}

beforeEach(() => {
  navigate.mockReset()
  location.pathname = `/t/acme/inbox`
})

describe(`SidebarRunningSection`, () => {
  it(`is hidden with nothing running`, () => {
    renderSection([groups[2]])
    expect(screen.queryByTestId(`sidebar-running`)).toBeNull()
  })

  it(`lists needs-you then running threads, never recent ones`, () => {
    renderSection()
    const rows = screen.getAllByTestId(/^sidebar-running-/)
    expect(rows.map((el) => el.getAttribute(`data-testid`))).toEqual([
      `sidebar-running-t-ask`,
      `sidebar-running-t-run`,
    ])
    expect(rows[0].textContent).toContain(`APP-5`)
    expect(rows[0].textContent).toContain(`Reduce cold start`)
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
  it(`hands the rail one item per running thread`, () => {
    const items: { key: string; label: string }[] = []
    render(
      <SidebarRunningIcons
        teamSlug="acme"
        threads={{ groups, extras }}
        renderItem={(item) => {
          items.push(item)
          return null
        }}
      />
    )
    expect(items.map((item) => [item.key, item.label])).toEqual([
      [`t-ask`, `APP-5 Reduce cold start`],
      [`t-run`, `Group board issues`],
    ])
  })
})

describe(`the nav state`, () => {
  it(`is green while anything runs, amber while something needs you`, () => {
    expect(threadsNavState([])).toEqual({ live: false, needsYou: false })
    expect(threadsNavState([groups[1], groups[2]])).toEqual({ live: true, needsYou: false })
    expect(threadsNavState(groups)).toEqual({ live: true, needsYou: true })
    expect(runningThreadRows([groups[2]])).toEqual([])
  })

  it(`maps a row onto the run mark`, () => {
    expect(threadRunMarkState({ needsYou: true, busy: true })).toBe(`needs_input`)
    expect(threadRunMarkState({ needsYou: false, busy: true })).toBe(`working`)
    expect(threadRunMarkState({ needsYou: false, busy: false })).toBeUndefined()
  })
})
