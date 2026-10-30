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
const location = vi.hoisted(
  () => ({ pathname: `/t/acme/inbox`, search: {} }) as { pathname: string; search: Record<string, unknown> }
)

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
  over: {
    busy?: boolean
    needsYou?: boolean
    deviceId?: string | null
    depth?: number
    parentId?: string | null
    rootId?: string
    unread?: boolean
  } = {}
) {
  return {
    sessionId,
    teamId: `team-1`,
    depth: over.depth ?? 0,
    parentId: over.parentId ?? null,
    rootId: over.rootId ?? sessionId,
    title,
    private: false,
    deviceId: over.deviceId ?? null,
    busy: over.busy ?? false,
    needsYou: over.needsYou ?? false,
    unread: over.unread ?? false,
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
  location.search = {}
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
    // EXP-1318: the root's device glyph trails the row (named for AT),
    // EXP-1477: in the rail's trailing column beside the button.
    expect(
      rows[0]
        .closest(`li`)
        ?.querySelector(`[data-slot="sidebar-menu-trailing"] [data-slot="session-row-device"]`)
        ?.getAttribute(`aria-label`)
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
    expect(threadsNavState([])).toEqual({ live: false, needsYou: false, unread: false })
    expect(threadsNavState([groups[1], groups[2]])).toEqual({
      live: true,
      needsYou: false,
      unread: false,
    })
    expect(threadsNavState(groups)).toEqual({ live: true, needsYou: true, unread: false })
  })

  it(`EXP-1323: carries the unread dot off a thread row`, () => {
    const recent: ThreadsListGroup = {
      key: `recent`,
      title: `Recent`,
      rows: [row(`t-new`, `Fix flaky scroll`, { unread: true })],
    }
    expect(threadsNavState([recent])).toEqual({ live: false, needsYou: false, unread: true })
    // #EXP-1388: an archived chat never lights the Chats dot.
    expect(
      threadsNavState([recent], [{ sessionId: `t-new`, readAt: `2026-01-01T00:00:00Z`, archivedAt: `2026-01-02T00:00:00Z` }])
    ).toEqual({ live: false, needsYou: false, unread: false })
  })

  it(`maps a row onto the run mark`, () => {
    expect(threadRunMarkState({ needsYou: true, busy: true })).toBe(`needs_input`)
    expect(threadRunMarkState({ needsYou: false, busy: true })).toBe(`working`)
    expect(threadRunMarkState({ needsYou: false, busy: false })).toBeUndefined()
  })
})

// EXP-1316: a row whose title is still held (its issue shapes pending) shows
// a skeleton line in the text's place, never a glyph with no text.
describe(`SidebarRunningSection — held titles`, () => {
  it(`renders a skeleton line for a held title`, () => {
    render(
      <SidebarProvider>
        <TooltipProvider>
          <SidebarRunningSection
            teamSlug="acme"
            threads={{
              groups: [
                {
                  key: `running`,
                  title: `Running`,
                  rows: [row(`t-held`, ``, { busy: true }), row(`t-run`, `Group board issues`, { busy: true })],
                },
              ],
              wordOf: () => null,
            }}
          />
        </TooltipProvider>
      </SidebarProvider>
    )
    expect(screen.getByTestId(`sidebar-running-title-skeleton`)).toBeTruthy()
    expect(screen.getByTestId(`sidebar-running-t-run`).textContent).toContain(`Group board issues`)
  })
})
