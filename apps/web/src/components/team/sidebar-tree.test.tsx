import { fireEvent, render, screen, within } from "@testing-library/react"
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest"
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
// #EXP-2124 (fixtures/sidebar-menus.json): the Assistant heads the root rows,
// and every row's right-click is its kind's menu off THE table, with real
// verbs (background tabs, rename in place, the archive confirm, Move to
// folder).

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
  useNavigate: () => h.navigate,
}))
vi.mock(`@/lib/collections`, () => ({ sessionCollection: { get: () => undefined }, sidebarLayoutCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    devices: { rename: { mutate: (...args: unknown[]) => h.renameDevice(...args) } },
    boards: { archive: { mutate: (...args: unknown[]) => h.archiveBoard(...args) } },
  },
}))
vi.mock(`@/hooks/use-work-tabs`, () => ({
  openNewRunPage: () => h.openNewRun(),
  openWorkTabInBackground: (tab: unknown) => h.backgroundTab(tab),
}))
vi.mock(`@/hooks/use-open-new-draft`, () => ({ useOpenNewDraft: () => h.newDraft }))
vi.mock(`@/components/chat/chat-menu`, () => ({
  useAgentSends: (agentId: string) => ({ compact: () => h.send(agentId, `compact`), "new-conversation": () => h.send(agentId, `new`) }),
}))
vi.mock(`@/components/team/sidebar-shared`, () => ({ NavCountBadge: () => null }))
vi.mock(`@/hooks/use-device-glyphs`, () => ({ useDeviceGlyphs: () => () => null }))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => [
    { id: `b-1`, slug: `mobile`, name: `Mobile App`, color: `#fff`, sortOrder: 1, icon: null, repositoryId: null },
  ],

  useTeamBoardsWithReady: () => ({ boardsReady: true, boards: [
    { id: `b-1`, slug: `mobile`, name: `Mobile App`, color: `#fff`, sortOrder: 1, icon: null, repositoryId: null },
  ] }),
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
const h = vi.hoisted(() => ({
  entries: [] as SidebarEntry[],
  rows: null as SidebarRow[] | null,
  addFolder: vi.fn(),
  move: vi.fn(),
  moveToNewFolder: vi.fn(),
  navigate: vi.fn(),
  renameDevice: vi.fn(),
  archiveBoard: vi.fn(),
  openNewRun: vi.fn(),
  backgroundTab: vi.fn(),
  newDraft: vi.fn(),
  send: vi.fn(),
  clipboard: vi.fn(),
}))
vi.mock(`@/hooks/use-sidebar-folders`, () => ({
  useSidebarFolders: (_teamId: string, _section: string, ids: (entries: SidebarEntry[]) => string[]) => ({
    rows:
      h.rows ??
      ids(h.entries).map(
        (id): SidebarRow => ({ kind: `item`, id, depth: 0, group: false, name: null, open: false, count: 0 })
      ),
    items: ids(h.entries),
    itemsOf: () => [],
    editingId: null,
    setEditingId: () => {},
    move: h.move,
    toggle: () => {},
    rename: () => {},
    ungroup: () => {},
    addFolder: h.addFolder,
    moveToNewFolder: h.moveToNewFolder,
    dropSession: () => {},
  }),
}))

import { SidebarRootRows, SidebarTeamsGroup, sidebarActive, type SidebarAssistant } from "@/components/team/sidebar-tree"
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

const bro: SidebarAssistant = {
  agentId: `a-x`,
  name: `Bro`,
  teamId: `t-acme`,
  teamSlug: `acme`,
  href: `/t/acme/agents/a-x`,
}

function renderRoot(needsYou: number, drafts: number, devices: SidebarDevice[] = [], assistant: SidebarAssistant | null = null) {
  return wrap(
    <SidebarRootRows
      active={sidebarActive(`/home`)}
      assistant={assistant}
      needsYou={needsYou}
      drafts={drafts}
      chatsBadge={{ count: 0, tone: `muted` }}
      devices={devices}
      teams={teams}
      onAddDevice={() => {}}
    />
  )
}

// Minimal in-memory Storage (as in sidebar-widths.test.ts): the runner's
// jsdom does not always ship a working localStorage.
function memoryStorage(): Storage {
  const map = new Map<string, string>()
  return {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => void map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  }
}

// Menus over Radix are slow to mount on a busy runner.
vi.setConfig({ testTimeout: 30_000 })

beforeAll(() => {
  Object.defineProperty(window, `localStorage`, { value: memoryStorage(), configurable: true })
})

beforeEach(() => {
  window.localStorage.clear()
  h.entries = []
  h.rows = null
  for (const fn of [h.addFolder, h.move, h.moveToNewFolder, h.navigate, h.renameDevice, h.archiveBoard, h.openNewRun, h.backgroundTab, h.newDraft, h.send, h.clipboard]) {
    fn.mockReset()
  }
  h.renameDevice.mockResolvedValue({})
  h.archiveBoard.mockResolvedValue({})
  h.clipboard.mockResolvedValue(undefined)
  Object.defineProperty(window.navigator, `clipboard`, { configurable: true, value: { writeText: h.clipboard } })
})

/** Right-clicks `element` and returns the open menu's row labels. */
function openMenu(element: Element): string[] {
  fireEvent.contextMenu(element, { clientX: 10, clientY: 10 })
  return menuLabels(screen.getByRole(`menu`))
}

function menuLabels(menu: HTMLElement): string[] {
  return [...menu.querySelectorAll(`[role^="menuitem"], [role="separator"]`)].map((el) =>
    el.getAttribute(`role`) === `separator` ? `—` : (el.textContent ?? ``).trim()
  )
}

function pick(label: string) {
  fireEvent.click(within(screen.getByRole(`menu`)).getByText(label))
}

describe(`#EXP-2027 the root rows`, () => {
  for (const c of fixture.root) {
    it(c.name, () => {
      renderRoot(c.needsYou, c.drafts, [], c.assistant ? bro : null)
      const shown = {
        assistant: !!screen.queryByTestId(`sidebar-assistant`),
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
    expect(rows[2].querySelector(`[data-slot=team-corner]`)?.getAttribute(`title`)).toBe(`Acme`)
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
    expect(within(row).getByTestId(`sidebar-team-badge`).textContent).toBe(`2`)
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

describe(`#EXP-2124 the root rows' menus`, () => {
  it(`the Assistant: its chat, the agent sends, Settings > Assistant; Open in new tab opens its tab in the background`, () => {
    renderRoot(0, 0, [], bro)
    const row = screen.getByTestId(`sidebar-assistant`)
    expect(row.getAttribute(`href`)).toBe(`/t/acme/agents/a-x`)
    expect(row.textContent).toBe(`Bro`)
    expect(openMenu(row)).toEqual([`Open`, `Open in new tab`, `—`, `Compact conversation`, `New conversation`, `—`, `Settings`])
    pick(`Open in new tab`)
    expect(h.backgroundTab).toHaveBeenCalledWith({ kind: `agent`, agentId: `a-x`, from: null, teamId: `t-acme`, teamSlug: `acme` })
    expect(h.navigate).not.toHaveBeenCalled()
    openMenu(screen.getByTestId(`sidebar-assistant`))
    pick(`Compact conversation`)
    expect(h.send).toHaveBeenCalledWith(`a-x`, `compact`)
    openMenu(screen.getByTestId(`sidebar-assistant`))
    pick(`Settings`)
    expect(h.navigate).toHaveBeenCalledWith({ href: `/settings/assistant` })
  })

  it(`Home: Open, Open in new tab (THE new session tab), New chat with its shortcut`, () => {
    renderRoot(0, 0)
    const labels = openMenu(screen.getByTestId(`sidebar-home`))
    expect(labels.slice(0, 3)).toEqual([`Open`, `Open in new tab`, `—`])
    expect(labels[3]).toMatch(/^New chat/)
    pick(`Open in new tab`)
    expect(h.backgroundTab).toHaveBeenCalledWith({ kind: `new`, from: null })
    openMenu(screen.getByTestId(`sidebar-home`))
    fireEvent.click(within(screen.getByRole(`menu`)).getByText(`New chat`))
    expect(h.openNewRun).toHaveBeenCalled()
  })

  it(`Chats opens as a page tab; Devices adds a device`, () => {
    const onAdd = vi.fn()
    wrap(
      <SidebarRootRows
        active={sidebarActive(`/home`)}
        assistant={null}
        needsYou={0}
        drafts={0}
        chatsBadge={{ count: 0, tone: `muted` }}
        devices={[]}
        teams={teams}
        onAddDevice={onAdd}
      />
    )
    openMenu(screen.getByTestId(`nav-chats`))
    pick(`Open in new tab`)
    expect(h.backgroundTab).toHaveBeenCalledWith({ kind: `page`, page: `chats`, from: null })
    expect(openMenu(screen.getByTestId(`sidebar-devices`).querySelector(`button`)!)).toEqual([`Add device`])
    pick(`Add device`)
    expect(onAdd).toHaveBeenCalled()
  })

  it(`my device: Rename in place (Enter saves through the device rename), Copy link, Settings`, async () => {
    renderRoot(0, 0, [device(`d-1`, `MacBook`, {})])
    const link = screen.getByTestId(`sidebar-device-row`).querySelector(`a`)!
    expect(openMenu(link)).toEqual([`Open`, `Open in new tab`, `—`, `Rename`, `Copy link`, `—`, `Settings`])
    pick(`Rename`)
    const input = (await screen.findByTestId(`sidebar-device-name`)) as HTMLInputElement
    expect(input.value).toBe(`MacBook`)
    fireEvent.change(input, { target: { value: `Laptop` } })
    fireEvent.keyDown(input, { key: `Enter` })
    expect(h.renameDevice).toHaveBeenCalledWith({ deviceId: `d-1`, label: `Laptop` }, expect.anything())
    expect(screen.queryByTestId(`sidebar-device-name`)).toBeNull()
    openMenu(screen.getByTestId(`sidebar-device-row`).querySelector(`a`)!)
    pick(`Copy link`)
    expect(h.clipboard).toHaveBeenCalledWith(`${window.location.origin}/devices/d-1`)
  })

  it(`a teammate's server: no Rename`, () => {
    renderRoot(0, 0, [device(`d-8`, `Acme CI`, { mine: false, teamId: `t-acme` })])
    expect(openMenu(screen.getByTestId(`sidebar-device-row`).querySelector(`a`)!)).toEqual([
      `Open`,
      `Open in new tab`,
      `—`,
      `Copy link`,
      `—`,
      `Settings`,
    ])
  })
})

describe(`#EXP-2124 a team's menus`, () => {
  it(`the team row's right-click: New › (THE New list), New folder, Settings`, () => {
    renderTeams()
    const row = within(screen.getByTestId(`sidebar-team-row`)).getByText(`Acme`)
    expect(openMenu(row)).toEqual([`New`, `New folder`, `—`, `Settings`])
    fireEvent.click(within(screen.getByRole(`menu`)).getByText(`New`))
    const subs = document.querySelectorAll<HTMLElement>(`[data-slot="dropdown-menu-sub-content"]`)
    expect(menuLabels(subs[subs.length - 1]!)).toEqual([`Chat`, `Agent`, `App`, `Board`])
  })

  it(`a board (owner): New issue on it, Move to folder, its settings and Archive behind the confirm`, async () => {
    renderTeams()
    const link = screen.getByTestId(`sidebar-board-row`).querySelector(`a`)!
    expect(openMenu(link)).toEqual([
      `Open`,
      `Open in new tab`,
      `—`,
      `New issue`,
      `—`,
      `Copy link`,
      `Move to folder`,
      `—`,
      `Settings`,
      `—`,
      `Archive`,
    ])
    pick(`Open in new tab`)
    expect(h.backgroundTab).toHaveBeenCalledWith({ kind: `board`, boardId: `b-1`, from: null, teamId: `t-acme`, teamSlug: `acme` })
    openMenu(screen.getByTestId(`sidebar-board-row`).querySelector(`a`)!)
    pick(`New issue`)
    expect(h.newDraft).toHaveBeenCalledWith({ boardId: `b-1` })
    openMenu(screen.getByTestId(`sidebar-board-row`).querySelector(`a`)!)
    pick(`Archive`)
    const confirm = screen.getByTestId(`sidebar-board-archive-confirm`)
    expect(h.archiveBoard).not.toHaveBeenCalled()
    fireEvent.click(within(confirm).getByRole(`button`, { name: `Archive` }))
    expect(h.archiveBoard).toHaveBeenCalledWith({ boardId: `b-1` })
  })

  it(`an agent: the agent sends and Move to folder; a placed session: Remove from sidebar`, () => {
    h.entries = [{ id: `e-1`, kind: `item`, targetId: `s-1`, parentId: null, name: null, sortOrder: 1 }]
    renderTeams()
    expect(openMenu(screen.getByTestId(`agents-row`))).toEqual([
      `Open`,
      `Open in new tab`,
      `—`,
      `Compact conversation`,
      `New conversation`,
      `—`,
      `Move to folder`,
      `—`,
      `Settings`,
    ])
    pick(`New conversation`)
    expect(h.send).toHaveBeenCalledWith(`a-1`, `new`)
    expect(openMenu(screen.getByTestId(`sidebar-session-row`).querySelector(`a`)!)).toEqual([
      `Open`,
      `Open in new tab`,
      `—`,
      `Copy link`,
      `Move to folder`,
      `Remove from sidebar`,
    ])
  })

  it(`Move to folder ›: the other folders, New folder, Top level for an item inside a folder`, () => {
    h.rows = [
      { kind: `folder`, id: `f-1`, depth: 0, group: true, name: `Work`, open: true, count: 1 },
      { kind: `item`, id: `b-1`, depth: 1, group: false, name: null, open: false, count: 0 },
      { kind: `folder`, id: `f-2`, depth: 0, group: true, name: `Later`, open: false, count: 0 },
      { kind: `item`, id: `p-1`, depth: 0, group: false, name: null, open: false, count: 0 },
    ]
    renderTeams()
    openMenu(screen.getByTestId(`sidebar-board-row`).querySelector(`a`)!)
    fireEvent.click(within(screen.getByRole(`menu`)).getByText(`Move to folder`))
    let subs = document.querySelectorAll<HTMLElement>(`[data-slot="dropdown-menu-sub-content"]`)
    expect(menuLabels(subs[subs.length - 1]!)).toEqual([`Later`, `—`, `New folder`, `Top level`])
    fireEvent.click(within(subs[subs.length - 1]!).getByText(`Top level`))
    expect(h.move).toHaveBeenCalledWith({ id: `b-1`, after: `f-1` })

    openMenu(screen.getByTestId(`apps-row`))
    fireEvent.click(within(screen.getByRole(`menu`)).getByText(`Move to folder`))
    subs = document.querySelectorAll<HTMLElement>(`[data-slot="dropdown-menu-sub-content"]`)
    expect(menuLabels(subs[subs.length - 1]!)).toEqual([`Work`, `Later`, `—`, `New folder`])
    fireEvent.click(within(subs[subs.length - 1]!).getByText(`New folder`))
    expect(h.moveToNewFolder).toHaveBeenCalledWith(`p-1`)
  })
})
