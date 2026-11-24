import { fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi, beforeEach } from "vitest"
import type { Issue } from "@/db/schema"

// EXP-907: the browser gesture on the work tabs — a MIDDLE click anywhere on
// a tab closes it, exactly like its ×, and the browser's own middle-click
// behaviour (autoscroll, open-in-new-tab) never fires. EVERY tab takes it.

const updateWorkTabs = vi.hoisted(() => vi.fn())
const navigate = vi.hoisted(() => vi.fn())
const T = { teamId: `t1`, teamSlug: `acme` }
const openNewRunPage = vi.hoisted(() => vi.fn())
const pathname = vi.hoisted(() => ({ value: `/t/acme/inbox` }))
const tabs = vi.hoisted(() => ({
  value: [] as unknown[],
}))
const issues = vi.hoisted(() => ({ value: [] as unknown[] }))
const apps = vi.hoisted(() => ({ value: [] as unknown[] }))
const devices = vi.hoisted(() => ({ value: [] as unknown[] }))
const assistant = vi.hoisted(() => ({ value: null as unknown }))
const memberships = vi.hoisted(() => ({
  myTeams: [] as unknown[],
  memberships: [] as unknown[],
}))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => navigate,
  useRouterState: () => pathname.value,
}))
// Each live query answers with the rows of the collection it reads.
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let source: { rows?: () => unknown[] } | undefined
      const chain: Record<string, unknown> = {}
      for (const verb of [`where`, `select`, `orderBy`, `limit`, `join`]) chain[verb] = () => chain
      const built = build({
        from: (sources: Record<string, { rows?: () => unknown[] }>) => {
          source = Object.values(sources)[0]
          return chain
        },
      })
      return { data: built === undefined ? undefined : (source?.rows?.() ?? []) }
    },
  }
})
vi.mock(`@/lib/collections`, () => ({
  issueCollection: { rows: () => issues.value },
  appCollection: { rows: () => apps.value },
  deviceCollection: { rows: () => devices.value },
  sessionCollection: { get: () => ({ cli: `claude` }), rows: () => [] },
}))
vi.mock(`@/hooks/use-sidebar-assistant`, () => ({ useSidebarAssistant: () => assistant.value }))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamMemberships: () => memberships,
}))
const chatGroups = vi.hoisted(() => ({ value: [] as unknown[] }))
const agents = vi.hoisted(() => ({ value: [] as unknown[] }))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useChatsList: () => ({
    groups: chatGroups.value,
    isReady: true,
  }),
  useAgents: () => ({ agents: agents.value, isReady: true }),
  useSessionTitles: () => ({ titles: new Map(), held: new Set(), ready: true }),
  useTeamSessions: () => ({ sessions: [], isReady: true }),
  useDigestMessages: () => ({ messages: [], isReady: true }),
  useSessionReads: () => new Map(),
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `me` } } }) }))
vi.mock(`@/hooks/use-work-tabs`, () => ({
  useWorkTabs: () => ({ tabs: tabs.value }),
  updateWorkTabs,
  openNewRunPage,
}))
vi.mock(`@/components/issue-properties/status-dropdown`, () => ({
  IssueStatusIcon: () => null,
}))

import { WorkTabsStrip } from "@/components/team/work-tabs-strip"

const issue = {
  id: `i1`,
  identifier: `EXP-1`,
  title: `Fix the sync loop`,
  boardId: `b1`,
} as unknown as Issue

/** jsdom's fireEvent has no `auxClick` helper — the real event, as a browser
 *  fires it. */
function auxClick(node: Element, button: number) {
  fireEvent(
    node,
    new MouseEvent(`auxclick`, { bubbles: true, cancelable: true, button })
  )
}

function renderStrip() {
  return render(
    <WorkTabsStrip
      teams={[
        { id: `t1`, slug: `acme` },
        { id: `t2`, slug: `beta` },
      ]}
      boards={[{ id: `b1`, slug: `web`, teamId: `t1` }] as never}
      team={null}
      teamSlug="acme"
    />
  )
}

describe(`WorkTabsStrip middle click (EXP-907)`, () => {
  beforeEach(() => {
    updateWorkTabs.mockClear()
    issues.value = [issue]
    pathname.value = `/t/acme/inbox`
  })

  it(`closes the tab on a middle click, and swallows the browser's own`, () => {
    tabs.value = [
      { kind: `issue`, issueId: `i1`, from: null, ...T },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i1`)

    // The mousedown is taken, or Chrome opens its autoscroll cursor first.
    const down = fireEvent.mouseDown(tab, { button: 1 })
    expect(down).toBe(false)

    auxClick(tab, 1)
    expect(updateWorkTabs).toHaveBeenCalledTimes(1)
    expect(typeof updateWorkTabs.mock.calls[0]![0]).toBe(`function`)
  })

  it(`leaves a left click and a right click alone`, () => {
    tabs.value = [
      { kind: `issue`, issueId: `i1`, from: null, ...T },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    fireEvent.mouseDown(tab, { button: 0 })
    auxClick(tab, 2)
    expect(updateWorkTabs).not.toHaveBeenCalled()
  })

  it(`closes a running chat's tab like every other`, () => {
    chatGroups.value = [
      {
        key: `running`,
        title: `Running`,
        rows: [{ sessionId: `t1`, teamId: `t1`, title: `Reduce cold start`, private: false, busy: true, needsYou: false, activityAt: `` }],
      },
    ]
    tabs.value = [{ kind: `chat`, chatId: `t1`, from: null, ...T }]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-chat:t1`)
    expect(screen.getAllByLabelText(`Close tab`).length).toBeGreaterThan(0)
    auxClick(tab, 1)
    expect(updateWorkTabs).toHaveBeenCalledTimes(1)
  })
})

// EXP-1255: the × shows on hover only (every tab, the active one too; a
// keyboard focus shows it), a "+" after the last tab opens the New run tab,
// and the New run chip is the Agent page's tab.
describe(`WorkTabsStrip New run (EXP-1255)`, () => {
  const issueTab = { kind: `issue`, issueId: `i1`, from: null, ...T }

  beforeEach(() => {
    updateWorkTabs.mockClear()
    openNewRunPage.mockClear()
    issues.value = [issue]
    pathname.value = `/t/acme/inbox`
  })

  it(`hides every × until the tab is hovered or the × is focused`, () => {
    tabs.value = [issueTab]
    pathname.value = `/t/acme/boards/web/issues/EXP-1`
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    expect(tab.getAttribute(`data-active`)).toBe(`true`)
    const close = tab.querySelector(`[aria-label="Close tab"]`)!
    expect(close.className).toContain(`opacity-0`)
    expect(close.className).toContain(`group-hover/tab:opacity-100`)
    expect(close.className).toContain(`focus-visible:opacity-100`)
  })

  it(`puts the "+" after the last tab and the "+N"`, () => {
    tabs.value = [issueTab]
    renderStrip()
    const plus = screen.getByTestId(`work-tabs-new-run`)
    expect(plus.getAttribute(`aria-label`)).toBe(`New tab`)
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    expect(tab.compareDocumentPosition(plus) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(plus.nextElementSibling).toBeNull()
  })

  it(`draws Home's chip (the New session tab), active on Home`, () => {
    tabs.value = [issueTab, { kind: `new`, from: null }]
    pathname.value = `/home`
    renderStrip()
    const chip = screen.getByTestId(`work-tab-new`)
    expect(chip.textContent).toContain(`Home`)
    expect(chip.textContent).not.toContain(`New session`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    expect(screen.getByTestId(`work-tab-issue:i1`).getAttribute(`data-active`)).toBeNull()
  })
})

// EXP-1293: chat chips read the Chats list (title + the agent mark),
// agent chips the agent's name; each is active on its own route.
describe(`WorkTabsStrip session-tree chips (EXP-1293)`, () => {
  beforeEach(() => {
    issues.value = []
    chatGroups.value = [
      {
        key: `needs_you`,
        title: `Needs you`,
        rows: [{ sessionId: `th1`, teamId: `t1`, title: `Reduce cold start`, private: false, busy: false, needsYou: true, activityAt: `` }],
      },
    ]
    agents.value = [
      { agentId: `ag1`, teamId: `t1`, name: `Triage`, icon: `bug`, color: `#a855f7`, busy: false, needsYou: false },
    ]
  })

  it(`titles a chat chip and marks it as needing you`, () => {
    tabs.value = [{ kind: `chat`, chatId: `th1`, from: null, ...T }]
    pathname.value = `/t/acme/sessions/th1`
    renderStrip()
    const chip = screen.getByTestId(`work-tab-chat:th1`)
    expect(chip.textContent).toContain(`Reduce cold start`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    expect(chip.querySelector(`[data-slot="run-mark"]`)?.getAttribute(`data-state`)).toBe(
      `needs_input`
    )
  })

  it(`names an agent chip, active on its channel`, () => {
    tabs.value = [{ kind: `agent`, agentId: `ag1`, from: null, ...T }]
    pathname.value = `/t/acme/agents/ag1`
    renderStrip()
    const chip = screen.getByTestId(`work-tab-agent:ag1`)
    expect(chip.textContent).toContain(`Triage`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
  })
})

// EXP-1259 D8: a tab whose subject has not synced yet keeps its chip but
// draws a short placeholder bar in the label slot, never "Loading…".
describe(`WorkTabsStrip unloaded subject (D8)`, () => {
  beforeEach(() => {
    issues.value = []
    pathname.value = `/t/acme/inbox`
  })

  it(`keeps the tab and draws the placeholder bar instead of text`, () => {
    tabs.value = [
      { kind: `issue`, issueId: `i9`, from: null, ...T },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i9`)
    expect(
      tab.querySelector(`[data-testid=work-tab-title-placeholder]`)
    ).not.toBeNull()
    expect(screen.queryByText(`Loading…`)).toBeNull()
  })
})

// #EXP-2027: one strip across every member team; a tab links into its own
// entity's team, whatever page is open.
describe(`WorkTabsStrip across teams (#EXP-2027)`, () => {
  beforeEach(() => {
    navigate.mockClear()
    issues.value = [issue]
    chatGroups.value = []
    pathname.value = `/home`
  })

  it(`draws every team's tabs and opens each in its own team`, () => {
    chatGroups.value = [
      {
        key: `recent`,
        title: `Recent`,
        rows: [{ sessionId: `c2`, teamId: `t2`, title: `Beta chat`, private: false, busy: false, needsYou: false, activityAt: `` }],
      },
    ]
    tabs.value = [
      { kind: `issue`, issueId: `i1`, from: null, ...T },
      { kind: `chat`, chatId: `c2`, from: null, teamId: `t2`, teamSlug: `old-beta` },
    ]
    renderStrip()
    fireEvent.click(within(screen.getByTestId(`work-tab-chat:c2`)).getByText(`Beta chat`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `beta`, sessionId: `c2` },
      search: {},
    })
    navigate.mockClear()
    fireEvent.click(within(screen.getByTestId(`work-tab-issue:i1`)).getByText(`Fix the sync loop`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: `acme`, boardSlug: `web`, issueIdentifier: `EXP-1` },
      search: {},
    })
  })

  it(`marks an issue active only under its own team's route`, () => {
    tabs.value = [{ kind: `issue`, issueId: `i1`, from: null, ...T }]
    pathname.value = `/t/beta/boards/web/issues/EXP-1`
    renderStrip()
    expect(screen.getByTestId(`work-tab-issue:i1`).getAttribute(`data-active`)).toBeNull()
  })
})

// #EXP-2124 (sidebar-menus.json `newTabCases`): the "+" opens THE new-tab
// menu: the Assistant first, a separator, then one submenu per team holding
// THE New list for that viewer.
describe(`WorkTabsStrip new-tab menu (#EXP-2124)`, () => {
  beforeEach(() => {
    navigate.mockClear()
    openNewRunPage.mockClear()
    tabs.value = []
    pathname.value = `/home`
    assistant.value = { agentId: `as1`, name: `Ada`, teamId: `t1`, teamSlug: `acme`, href: `/t/acme/agents/as1` }
    memberships.myTeams = [
      { id: `t1`, slug: `acme`, name: `Acme`, iconUrl: null, kind: `team` },
      { id: `t2`, slug: `beta`, name: `Beta`, iconUrl: null, kind: `team` },
    ]
    memberships.memberships = [
      { teamId: `t1`, role: `member` },
      { teamId: `t2`, role: `owner` },
    ]
  })

  function openPlus() {
    renderStrip()
    fireEvent.pointerDown(screen.getByTestId(`work-tabs-new-run`), { button: 0, pointerType: `mouse` })
  }

  function submenuRows(teamId: string): string[] {
    const trigger = screen.getByTestId(`new-tab-team-${teamId}`)
    trigger.focus()
    fireEvent.keyDown(trigger, { key: `ArrowRight` })
    return screen.getAllByTestId(/^sidebar-new-/).map((row) => row.textContent ?? ``)
  }

  it(`lists the Assistant, then one submenu per team`, () => {
    openPlus()
    expect(openNewRunPage).not.toHaveBeenCalled()
    const rows = screen.getAllByRole(`menuitem`).map((node) => node.textContent)
    expect(rows).toEqual([`Ada`, `Acme`, `Beta`])
    expect(screen.getAllByRole(`separator`).length).toBe(1)
    fireEvent.click(screen.getByTestId(`new-tab-assistant`))
    expect(navigate).toHaveBeenCalledWith({ href: `/t/acme/agents/as1` })
  })

  it(`holds THE New list: Chat for a member`, () => {
    openPlus()
    expect(submenuRows(`t1`)).toEqual([`Chat`])
  })

  it(`holds THE New list: Chat, Agent, App, Board for an owner`, () => {
    openPlus()
    expect(submenuRows(`t2`)).toEqual([`Chat`, `Agent`, `App`, `Board`])
  })

  it(`has no Assistant row while the viewer has none`, () => {
    assistant.value = null
    openPlus()
    expect(screen.getAllByRole(`menuitem`).map((node) => node.textContent)).toEqual([`Acme`, `Beta`])
    expect(screen.queryByTestId(`new-tab-assistant`)).toBeNull()
  })
})

// #EXP-2124: pages are tabs: a board, an app, a device and the list pages,
// each THE strip chip with its own lead + name, active on its own route.
describe(`WorkTabsStrip page chips (#EXP-2124)`, () => {
  beforeEach(() => {
    navigate.mockClear()
    issues.value = []
    apps.value = [{ id: `a1`, slug: `notes`, name: `Notes`, icon: null, color: `#22c55e`, teamId: `t1`, deletedAt: null }]
    devices.value = [{ id: `row-1`, deviceId: `d1`, label: `Studio Mac`, kind: `desktop` }]
    pathname.value = `/home`
  })

  function renderWithBoard() {
    return render(
      <WorkTabsStrip
        teams={[{ id: `t1`, slug: `acme` }]}
        boards={[{ id: `b1`, slug: `web`, teamId: `t1`, name: `Web`, color: `#ef4444`, icon: null }] as never}
        team={null}
        teamSlug="acme"
      />
    )
  }

  it(`draws a board chip: its icon in its color + its name, and opens its page`, () => {
    tabs.value = [{ kind: `board`, boardId: `b1`, from: null, ...T }]
    pathname.value = `/t/acme/boards/web`
    renderWithBoard()
    const chip = screen.getByTestId(`work-tab-board:b1`)
    expect(chip.textContent).toContain(`Web`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    expect(chip.querySelector(`svg`)?.getAttribute(`style`)).toContain(`color`)
    fireEvent.click(within(chip).getByText(`Web`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug: `acme`, boardSlug: `web` },
      search: {},
    })
  })

  it(`draws an app chip, active on any of its screens`, () => {
    tabs.value = [{ kind: `app`, appId: `a1`, from: null, ...T }]
    pathname.value = `/t/acme/apps/notes/list`
    renderWithBoard()
    const chip = screen.getByTestId(`work-tab-app:a1`)
    expect(chip.textContent).toContain(`Notes`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    fireEvent.click(within(chip).getByText(`Notes`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/apps/$appSlug`,
      params: { teamSlug: `acme`, appSlug: `notes` },
      search: {},
    })
  })

  it(`draws a device chip with its name, active on its page`, () => {
    tabs.value = [{ kind: `device`, deviceId: `d1`, from: null }]
    pathname.value = `/devices/d1`
    renderWithBoard()
    const chip = screen.getByTestId(`work-tab-device:d1`)
    expect(chip.textContent).toContain(`Studio Mac`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    fireEvent.click(within(chip).getByText(`Studio Mac`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/devices/$deviceId`,
      params: { deviceId: `d1` },
      search: {},
    })
  })

  it(`draws the list pages' chips with their nav labels`, () => {
    tabs.value = [
      { kind: `page`, page: `needs-you`, from: null },
      { kind: `page`, page: `chats`, from: null },
      { kind: `page`, page: `drafts`, from: null },
    ]
    pathname.value = `/chats`
    renderWithBoard()
    expect(screen.getByTestId(`work-tab-page:needs-you`).textContent).toContain(`Needs you`)
    expect(screen.getByTestId(`work-tab-page:drafts`).textContent).toContain(`Drafts`)
    const chats = screen.getByTestId(`work-tab-page:chats`)
    expect(chats.textContent).toContain(`Chats`)
    expect(chats.getAttribute(`data-active`)).toBe(`true`)
    expect(screen.getByTestId(`work-tab-page:drafts`).getAttribute(`data-active`)).toBeNull()
  })

  it(`keeps an unknown board, app or device as a placeholder`, () => {
    apps.value = []
    devices.value = []
    tabs.value = [
      { kind: `board`, boardId: `b9`, from: null, ...T },
      { kind: `app`, appId: `a9`, from: null, ...T },
      { kind: `device`, deviceId: `d9`, from: null },
    ]
    renderWithBoard()
    for (const key of [`board:b9`, `app:a9`, `device:d9`]) {
      expect(
        screen.getByTestId(`work-tab-${key}`).querySelector(`[data-testid=work-tab-title-placeholder]`),
        key
      ).not.toBeNull()
    }
  })
})
