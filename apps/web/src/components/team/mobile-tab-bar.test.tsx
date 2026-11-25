import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/chats.json"
import type { Board } from "@/db/schema"

// #EXP-2109 (chats.json `shell.dock`): the docked bar over every member team.

const h = vi.hoisted(() => ({
  pathname: `/chats`,
  params: {} as Record<string, string>,
  navigate: vi.fn(),
  scope: undefined as readonly string[] | undefined,
  chats: { count: 0, tone: `muted` } as { count: number; tone: `attention` | `muted` },
  boards: [] as unknown[],
  last: null as { teamSlug: string; boardSlug?: string } | null,
  bulk: false,
}))

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children, to, ...props }: { children?: unknown; to?: string } & Record<string, unknown>) => (
    <a href={to} {...(props as Record<string, never>)}>
      {children as never}
    </a>
  ),
  useParams: () => h.params,
  useSearch: () => ({}),
  useNavigate: () => h.navigate,
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) => select({ location: { pathname: h.pathname } }),
}))
vi.mock(`@/lib/last-visited`, () => ({ readLastVisited: () => h.last }))
vi.mock(`@/hooks/use-chrome-height-var`, () => ({ useChromeHeightVar: () => () => {} }))
vi.mock(`@/hooks/use-mobile-chrome`, () => ({ useMobileChrome: () => ({ bulkBarPresent: h.bulk }) }))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `u-1` } } }) }))
vi.mock(`@/hooks/use-nav-counts`, () => ({
  useChatsBadge: (teamIds: readonly string[]) => {
    h.scope = teamIds
    return h.chats
  },
}))
vi.mock(`@/components/home/use-member-teams`, () => ({
  useMemberTeams: () => ({
    teams: [
      { id: `t1`, slug: `acme` },
      { id: `t2`, slug: `globex` },
    ],
    teamIds: [`t1`, `t2`],
  }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({ useBoardsForTeams: () => ({ boards: h.boards, boardsReady: true }) }))

import { MobileTabBar, newIssueBoard } from "@/components/team/mobile-tab-bar"

const web = { id: `b1`, slug: `web`, teamId: `t1` } as Board
const ios = { id: `b2`, slug: `ios`, teamId: `t2` } as Board
const teams = [
  { id: `t1`, slug: `acme` },
  { id: `t2`, slug: `globex` },
]
const IDS = fixture.testIds

beforeEach(() => {
  Object.assign(h, { pathname: `/chats`, params: {}, scope: undefined, chats: { count: 0, tone: `muted` }, boards: [web, ios], last: null, bulk: false })
  h.navigate.mockReset()
})

describe(`MobileTabBar`, () => {
  it(`draws the five cells in order and links four of them`, () => {
    render(<MobileTabBar />)
    const cells = [...document.querySelectorAll(`[data-slot=phone-dock-tab]`)]
    expect(cells.map((cell) => cell.getAttribute(`aria-label`))).toEqual(
      fixture.shell.tabs.map((tab) => fixture.copy.tabs[tab as keyof typeof fixture.copy.tabs])
    )
    expect(cells.map((cell) => cell.getAttribute(`href`))).toEqual([`/chats`, `/teams`, `/assistant`, null, `/settings`])
  })

  it.each([
    [`/chats`, IDS.tabChats],
    [`/t/acme/boards/web`, IDS.tabTeams],
    [`/t/acme/settings/general`, IDS.tabTeams],
    [`/assistant`, IDS.tabAssistant],
    [`/devices`, IDS.tabSettings],
  ])(`%s lights %s`, (pathname, testId) => {
    h.pathname = pathname
    render(<MobileTabBar />)
    const lit = [...document.querySelectorAll(`[aria-current=page]`)].map((cell) => cell.getAttribute(`data-testid`))
    expect(lit).toEqual([testId])
  })

  it(`hides on a detail and under a selection`, () => {
    h.pathname = `/t/acme/sessions/s-1`
    const { container, unmount } = render(<MobileTabBar />)
    expect(container.firstChild).toBeNull()
    unmount()
    h.pathname = `/chats`
    h.bulk = true
    expect(render(<MobileTabBar />).container.firstChild).toBeNull()
  })

  it(`badges Chats over every team`, () => {
    h.chats = { count: 3, tone: `attention` }
    render(<MobileTabBar />)
    expect(h.scope).toEqual([`t1`, `t2`])
    expect(screen.getByTestId(IDS.tabChats).textContent).toContain(`3`)
  })

  it(`New opens the popup: Chat opens the composer, Issue a draft on the target board`, () => {
    h.last = { teamSlug: `globex`, boardSlug: `ios` }
    render(<MobileTabBar />)
    expect(screen.queryByTestId(IDS.newMenu)).toBeNull()
    fireEvent.click(screen.getByTestId(IDS.tabNew))
    fireEvent.click(screen.getAllByTestId(IDS.newMenuRow)[1]!)
    expect(h.navigate).toHaveBeenLastCalledWith({ to: `/home` })
    fireEvent.click(screen.getByTestId(IDS.tabNew))
    fireEvent.click(screen.getAllByTestId(IDS.newMenuRow)[0]!)
    const call = h.navigate.mock.lastCall![0] as { params: { teamSlug: string }; search: { board?: string } }
    expect(call.params.teamSlug).toBe(`globex`)
    expect(call.search.board).toBe(`b2`)
  })

  it(`dims Issue while the viewer has no board`, () => {
    h.boards = []
    render(<MobileTabBar />)
    fireEvent.click(screen.getByTestId(IDS.tabNew))
    expect((screen.getAllByTestId(IDS.newMenuRow)[0] as HTMLButtonElement).disabled).toBe(true)
  })
})

describe(`newIssueBoard`, () => {
  const boards = [web, ios]
  it(`is the board on screen, else the last used, else the first`, () => {
    expect(newIssueBoard({ boards, teams, onScreen: { teamSlug: `globex`, boardSlug: `ios` }, last: { teamSlug: `acme`, boardSlug: `web` } })).toEqual({ board: ios, teamSlug: `globex` })
    expect(newIssueBoard({ boards, teams, onScreen: null, last: { teamSlug: `globex`, boardSlug: `ios` } })).toEqual({ board: ios, teamSlug: `globex` })
    expect(newIssueBoard({ boards, teams, onScreen: null, last: { teamSlug: `globex`, boardSlug: `gone` } })).toEqual({ board: web, teamSlug: `acme` })
    expect(newIssueBoard({ boards: [], teams, onScreen: null, last: null })).toBeNull()
  })
})
