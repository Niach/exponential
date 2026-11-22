import { fireEvent, render, screen, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SidebarProvider, TooltipProvider } from "@exp/ui"
import fixture from "@exp/domain-contract/fixtures/sidebar-tree.json"
import type { SidebarEntry, SidebarRow } from "@exp/domain-contract/sidebar-folders"
import { sidebarTeamOpenKey, SIDEBAR_DEVICES_OPEN_KEY } from "@exp/domain-contract/sidebar-tree"

// #EXP-2027 (fixtures/sidebar-tree.json): the web md+ main menu as ONE tree.
// Locks: the root rows are `sidebarRootRows` (Needs you and Drafts only while
// they count), Devices folds on a press (no link) and lists each device
// (mine with the online dot, a teammate's server with its team's mark, offline
// muted), and a team row folds on a press, carries its needs-you badge and
// the `…` menu, and lists its ONE mixed list (`sidebarTeamItems`).

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({
    to,
    params,
    children,
    ...rest
  }: {
    to: string
    params?: Record<string, string>
    search?: unknown
    children: React.ReactNode
  }) => {
    let href = to
    for (const [key, value] of Object.entries(params ?? {})) href = href.replace(`$${key}`, value)
    return (
      <a href={href} {...rest}>
        {children}
      </a>
    )
  },
  useNavigate: () => vi.fn(),
}))
vi.mock(`@/lib/collections`, () => ({ sessionCollection: { get: () => undefined }, sidebarLayoutCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/components/team/sidebar-shared`, () => ({ NavCountBadge: () => null }))
vi.mock(`@/hooks/use-device-glyphs`, () => ({ useDeviceGlyphs: () => () => null }))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => [
    { id: `b-1`, slug: `mobile`, name: `Mobile App`, color: `#fff`, sortOrder: 1, icon: null, repositoryId: null },
  ],
}))
vi.mock(`@/hooks/use-apps`, () => ({
  useApps: () => ({ apps: [{ id: `p-1`, slug: `crm`, name: `CRM`, icon: null, color: null, row: null }] }),
}))
vi.mock(`@/hooks/use-sidebar-teams-data`, () => {
  const slice = {
    agents: [
      { agentId: `a-x`, teamId: `t-acme`, name: `Assistant`, icon: ``, color: ``, busy: false, needsYou: false, assistant: true },
      { agentId: `a-1`, teamId: `t-acme`, name: `Triage`, icon: ``, color: ``, busy: false, needsYou: false, assistant: false },
    ],
    badges: new Map(),
    sessions: [{ id: `s-1`, teamId: `t-acme`, parentId: null }],
    titles: new Map([[`s-1`, `Store screenshots`]]),
    chatRows: new Map(),
  }
  return { useSidebarTeamsData: () => new Map([[`t-acme`, slice]]), sidebarTeamSlice: (data: Map<string, unknown>, id: string) => data.get(id) }
})
const h = vi.hoisted(() => ({ entries: [] as SidebarEntry[], addFolder: vi.fn() }))
vi.mock(`@/hooks/use-sidebar-folders`, () => ({
  useSidebarFolders: (_teamId: string, _section: string, ids: (entries: SidebarEntry[]) => string[]) => ({
    rows: ids(h.entries).map(
      (id): SidebarRow => ({ kind: `item`, id, depth: 0, group: false, name: null, open: false, count: 0 })
    ),
    items: ids(h.entries),
    editingId: null,
    setEditingId: () => {},
    move: () => {},
    toggle: () => {},
    rename: () => {},
    ungroup: () => {},
    addFolder: h.addFolder,
    dropSession: () => {},
  }),
}))

import { SidebarRootRows, SidebarTeamsGroup, sidebarActive } from "@/components/team/sidebar-tree"
import type { SidebarDevice } from "@/hooks/use-sidebar-tree"
import type { Team } from "@/db/schema"

const acme = { id: `t-acme`, slug: `acme`, name: `Acme`, iconUrl: null } as unknown as Team
const teams = new Map([[acme.id, acme]])

const device = (id: string, name: string, over: Partial<SidebarDevice>): SidebarDevice =>
  ({ id, name, mine: true, muted: false, teamId: null, device: { deviceId: id }, ...over }) as SidebarDevice

function wrap(node: React.ReactNode) {
  return render(
    <TooltipProvider>
      <SidebarProvider>{node}</SidebarProvider>
    </TooltipProvider>
  )
}

function renderRoot(needsYou: number, drafts: number, devices: SidebarDevice[] = []) {
  return wrap(
    <SidebarRootRows
      active={sidebarActive(`/home`)}
      needsYou={needsYou}
      drafts={drafts}
      chatsBadge={{ count: 0, tone: `muted` }}
      devices={devices}
      teams={teams}
      onAddDevice={() => {}}
    />
  )
}

beforeEach(() => {
  window.localStorage.clear()
  h.entries = []
  h.addFolder.mockClear()
})

describe(`#EXP-2027 the root rows`, () => {
  for (const c of fixture.root) {
    it(c.name, () => {
      renderRoot(c.needsYou, c.drafts)
      const shown = {
        home: !!screen.queryByTestId(`sidebar-home`),
        "needs-you": !!screen.queryByTestId(`needs-you-count-badge`),
        chats: !!screen.queryByTestId(`nav-chats`),
        drafts: !!screen.queryByTestId(`nav-drafts`),
        devices: !!screen.queryByTestId(`sidebar-devices`),
      }
      expect(Object.entries(shown).filter(([, on]) => on).map(([id]) => id)).toEqual(c.expected)
    })
  }

  it(`Home links to /home and is active there`, () => {
    renderRoot(0, 0)
    const home = screen.getByTestId(`sidebar-home`)
    expect(home.getAttribute(`href`)).toBe(`/home`)
    expect(home.getAttribute(`data-active`)).toBe(`true`)
  })

  it(`Devices folds on a press and lists each device`, () => {
    renderRoot(0, 0, [
      device(`d-1`, `MacBook`, {}),
      device(`d-2`, `Windows`, { muted: true }),
      device(`d-8`, `Acme CI`, { mine: false, teamId: `t-acme` }),
    ])
    const devices = screen.getByTestId(`sidebar-devices`)
    expect(devices.querySelector(`a`)).toBeNull()
    const rows = screen.getAllByTestId(`sidebar-device-row`)
    expect(rows.map((row) => row.textContent)).toEqual([`MacBook`, `Windows`, `Acme CI`])
    expect(rows[0].querySelector(`a`)?.getAttribute(`href`)).toBe(`/devices/d-1`)
    expect(rows[0].querySelector(`[data-slot="live-dot"]`)).toBeTruthy()
    expect(rows[1].getAttribute(`data-muted`)).toBe(`true`)
    expect(rows[1].querySelector(`[data-slot="live-dot"]`)).toBeNull()
    expect(within(rows[2]).getByTestId(`sidebar-device-team`).getAttribute(`title`)).toBe(`Acme`)
    expect(within(rows[2]).getByLabelText(/Device settings/).getAttribute(`href`)).toBe(`/devices/d-8/settings`)

    fireEvent.click(devices.querySelector(`button`)!)
    expect(window.localStorage.getItem(SIDEBAR_DEVICES_OPEN_KEY)).toBe(`0`)
    expect(screen.queryAllByTestId(`sidebar-device-row`)).toHaveLength(0)
  })
})

function renderTeams(needsYou = 0) {
  return wrap(
    <SidebarTeamsGroup
      teams={[acme]}
      owners={new Set([acme.id])}
      needsYou={new Map([[acme.id, needsYou]])}
      chatRows={new Map()}
      me="u-1"
      active={sidebarActive(`/t/acme/boards/mobile`)}
      onNewTeam={() => {}}
      onJoinTeam={() => {}}
    />
  )
}

describe(`#EXP-2027 a team`, () => {
  it(`lists its one mixed list under its row: boards, agents (no Assistant), apps, placed sessions`, () => {
    h.entries = [{ id: `e-1`, kind: `item`, targetId: `s-1`, parentId: null, name: null, sortOrder: 1 }]
    renderTeams()
    const list = screen.getByTestId(`sidebar-team-items`)
    const keys = [...list.querySelectorAll(`[data-reorder-key]`)].map((el) => el.getAttribute(`data-reorder-key`))
    expect(keys).toEqual([`t-acme`, `b-1`, `a-1`, `p-1`, `s-1`])
    expect(list.getAttribute(`data-sidebar-team-list`)).toBe(`t-acme`)
    // The team row heads the List at depth 0, its nodes one deeper.
    expect(screen.getByTestId(`sidebar-team-row`).getAttribute(`data-reorder-depth`)).toBe(`0`)
    const board = screen.getByTestId(`sidebar-board-row`)
    expect(board.getAttribute(`data-reorder-depth`)).toBe(`1`)
    expect(board.querySelector(`a`)?.getAttribute(`href`)).toBe(`/t/acme/boards/mobile`)
    expect(board.querySelector(`a`)?.getAttribute(`data-active`)).toBe(`true`)
    expect(screen.getByTestId(`sidebar-session-row`).querySelector(`a`)?.getAttribute(`href`)).toBe(
      `/t/acme/sessions/s-1`
    )
    expect(screen.getByTestId(`sidebar-session-row`).textContent).toContain(`Store screenshots`)
  })

  it(`folds on a press (no link) and shows its needs-you count and the … menu`, () => {
    renderTeams(2)
    const row = screen.getByTestId(`sidebar-team-row`)
    expect(row.querySelector(`a`)).toBeNull()
    expect(within(row).getByTestId(`sidebar-team-needs-you`).textContent).toBe(`2`)
    expect(within(row).getByTestId(`sidebar-team-menu-button`)).toBeTruthy()
    fireEvent.click(within(row).getByText(`Acme`))
    expect(window.localStorage.getItem(sidebarTeamOpenKey(`t-acme`))).toBe(`0`)
    expect(screen.queryByTestId(`sidebar-board-row`)).toBeNull()
  })
})

describe(`sidebarActive`, () => {
  it(`reads the row a path lights up`, () => {
    expect(sidebarActive(`/t/acme/apps/crm/home`)).toMatchObject({ teamSlug: `acme`, app: `crm`, board: null })
    expect(sidebarActive(`/t/acme/agents/a-1`)).toMatchObject({ agent: `a-1` })
    expect(sidebarActive(`/devices/d-1/settings`)).toMatchObject({ device: `d-1`, teamSlug: null })
  })
})
