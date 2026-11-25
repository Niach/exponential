import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Board, Team } from "@/db/schema"

// EXP-973: the phone's FAB is the SAME split capsule on EVERY tab-bar route.
// Devices, Actions/Automations, Chats, Inbox and Support used to drop the
// New-issue arm and leave a lone chat circle, which turned "file this" into a
// trip back to a board. Nothing collapses it any more: a team with NO board
// dims the arm in place (×3 with iOS / Android `composeEnabled`), so the
// control never moves under the thumb.

const route = vi.hoisted(() => ({ value: `` as string }))
const navigate = vi.hoisted(() => vi.fn())

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({
    children,
    to,
    ...props
  }: {
    children?: unknown
    to?: string
  } & Record<string, unknown>) => (
    <a href={to} {...(props as Record<string, never>)}>
      {children as never}
    </a>
  ),
  useParams: () => ({}),
  useSearch: () => ({}),
  useNavigate: () => navigate,
  useRouter: () => ({
    state: { location: { pathname: `/t/acme/devices`, search: {} } },
  }),
  // `matchRoute({ to })` — the bar only ever asks whether a route is active.
  useMatchRoute: () => (args: { to: string }) => args.to === route.value,
}))
vi.mock(`@/lib/last-visited`, () => ({ readLastVisited: () => null }))
vi.mock(`@/hooks/use-chrome-height-var`, () => ({
  useChromeHeightVar: () => () => {},
}))
vi.mock(`@/hooks/use-mobile-chrome`, () => ({
  useMobileChrome: () => ({ bulkBarPresent: false }),
}))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))
const inbox = vi.hoisted(() => ({
  unread: 0,
  drafts: [] as unknown[],
  draftTeamId: undefined as string | undefined,
}))
vi.mock(`@/hooks/use-unread-notifications`, () => ({
  useUnreadNotificationCount: () => inbox.unread,
}))
vi.mock(`@/hooks/use-issue-drafts`, () => ({
  useDraftEntries: (teamId: string | undefined) => {
    inbox.draftTeamId = teamId
    return inbox.drafts
  },
}))
const chatsNav = vi.hoisted(() => ({
  value: { count: 0, tone: `muted` } as { count: number; tone: `attention` | `muted` },
  scope: undefined as string | readonly string[] | undefined,
}))
vi.mock(`@/hooks/use-nav-counts`, () => ({
  useChatsBadge: (teamIds: string | readonly string[] | undefined) => {
    chatsNav.scope = teamIds
    return chatsNav.value
  },
}))
// EXP-1186: the bar's cross-team scope — the member teams the test sets.
const memberTeams = vi.hoisted(() => ({ value: null as unknown[] | null }))
vi.mock(`@/hooks/use-cross-team-scope`, () => ({
  useCrossTeamScope: (active: { id: string } | null) => {
    const teams = (memberTeams.value ?? (active ? [active] : [])) as {
      id: string
    }[]
    return {
      teams,
      teamIds: teams.map((t) => t.id).sort(),
      grouped: teams.length > 1,
    }
  },
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useBoardsForTeams: () => ({ boards: [], boardsReady: true }),
  useTeamMemberships: () => ({
    myTeams: [
      { id: `t1`, name: `Acme`, slug: `acme`, iconUrl: null },
      { id: `t2`, name: `Globex`, slug: `globex`, iconUrl: null },
    ],
  }),
}))
vi.mock(`@/components/create-team-dialog`, () => ({ CreateTeamDialog: () => null }))
const perms = vi.hoisted(() => ({ canCreate: true }))
vi.mock(`@/hooks/use-team-permissions`, () => ({
  useTeamPermissions: () => perms,
}))
vi.mock(`@/components/create-board-dialog`, () => ({
  CreateBoardDialog: ({ open }: { open: boolean }) =>
    open ? <div data-testid="create-board-dialog" /> : null,
}))

import { MobileTabBar } from "@/components/team/mobile-tab-bar"

const team = { id: `t1`, name: `Acme` } as Team
const boards = [
  { id: `b1`, slug: `web`, name: `Web` },
  { id: `b2`, slug: `ios`, name: `iOS` },
] as Board[]

function renderBar(
  boardList: Board[] | undefined = boards,
  teamRow: Team = team
) {
  return render(
    <MobileTabBar teamSlug="acme" team={teamRow} boards={boardList} />
  )
}

describe(`MobileTabBar FAB (EXP-973)`, () => {
  beforeEach(() => {
    route.value = ``
  })

  it(`draws both arms on every tab-bar route`, () => {
    for (const path of [
      ``,
      `/t/$teamSlug`,
      `/t/$teamSlug/agent`,
      `/t/$teamSlug/boards/$boardSlug`,
      `/t/$teamSlug/devices`,
      `/t/$teamSlug/chats`,
    ]) {
      route.value = path
      const view = renderBar()
      expect(screen.getByTestId(`fab-group`), path).toBeTruthy()
      expect(screen.getByTestId(`chat-button`), path).toBeTruthy()
      expect(screen.getByTestId(`compose-button`), path).toBeTruthy()
      view.unmount()
    }
  })

  // EXP-1170: the arm opens the New issue PAGE on a draft minted at tap
  // time, filing onto the fallback board off a board route.
  it(`points the New issue arm at the fallback board off a board route`, () => {
    route.value = `/t/$teamSlug/devices`
    navigate.mockReset()
    renderBar()
    fireEvent.click(screen.getByTestId(`compose-button`))
    expect(navigate).toHaveBeenCalledTimes(1)
    const target = navigate.mock.calls[0][0]
    expect(target.to).toBe(`/t/$teamSlug/drafts/$draftId`)
    expect(target.params.teamSlug).toBe(`acme`)
    expect(target.params.draftId).toMatch(/^[0-9a-f-]{36}$/)
    expect(target.search).toEqual({ board: `b1` })
  })

  it(`dims the New issue arm in place when the team has no board`, () => {
    route.value = `/t/$teamSlug/devices`
    renderBar([])
    // The capsule and BOTH arms stay — only the target is missing.
    expect(screen.getByTestId(`fab-group`)).toBeTruthy()
    expect(screen.getByTestId(`chat-button`).getAttribute(`href`)).toBe(
      `/t/$teamSlug/agent`
    )
    const compose = screen.getByTestId(`compose-button`)
    expect(compose.getAttribute(`aria-disabled`)).toBe(`true`)
    expect(compose.getAttribute(`aria-label`)).toBe(`New issue (no board)`)
    expect(compose.getAttribute(`href`)).toBeNull()
    expect(compose.className).toContain(`pointer-events-none`)
    expect(compose.className).toContain(`opacity-50`)
  })

  it(`leaves the arm live wherever a board resolves`, () => {
    route.value = `/t/$teamSlug/chats`
    renderBar()
    const compose = screen.getByTestId(`compose-button`)
    expect(compose.getAttribute(`aria-disabled`)).toBeNull()
    expect(compose.getAttribute(`aria-label`)).toBe(`New issue`)
  })
})

// EXP-1293 → #FEED-232: Chats — always a tab, wearing THE count badge
// (`chatsBadge`); phones read the ONE active team (chats.json `shell`).
describe(`MobileTabBar Chats (chats.json)`, () => {
  beforeEach(() => {
    route.value = ``
    memberTeams.value = null
    chatsNav.value = { count: 0, tone: `muted` }
  })

  const chatsTab = () => screen.getByTestId(`tab-chats`)

  it(`links Chats and lights it on its routes`, () => {
    route.value = `/t/$teamSlug/chats`
    renderBar()
    expect(chatsTab().getAttribute(`href`)).toBe(`/t/$teamSlug/chats`)
    expect(chatsTab().className).toContain(`bg-glass-active`)
    expect(chatsTab().getAttribute(`aria-current`)).toBe(`page`)
  })

  it(`badges Chats with its count, attention while one needs you`, () => {
    const view = renderBar()
    expect(chatsTab().querySelector(`[data-slot=phone-tab-badge]`)).toBeNull()
    view.unmount()
    chatsNav.value = { count: 3, tone: `muted` }
    const unread = renderBar()
    const badge = chatsTab().querySelector(`[data-slot=phone-tab-badge]`)!
    expect(badge.textContent).toBe(`3`)
    expect(badge.getAttribute(`data-tone`)).toBe(`muted`)
    unread.unmount()
    chatsNav.value = { count: 2, tone: `attention` }
    renderBar()
    expect(chatsTab().querySelector(`[data-slot=phone-tab-badge]`)!.getAttribute(`data-tone`)).toBe(`attention`)
  })

  it(`reads the badge off the active team only`, () => {
    memberTeams.value = [team, { id: `t2`, name: `Beta` }]
    renderBar()
    expect(chatsNav.scope).toBe(`t1`)
  })

  // EXP-1327: the count rides Chats, never the chat arm.
  it(`never badges the chat arm`, () => {
    chatsNav.value = { count: 2, tone: `attention` }
    renderBar()
    expect(screen.getByTestId(`chat-button`).querySelector(`[data-slot=badge], [data-slot=phone-tab-badge]`)).toBeNull()
  })
})

// chats.json `shell.tabs` (#EXP-1384): Board · Chats · Apps · Team, each an
// icon over its label; the Board tab = the active board's glyph + name, the
// Team tab = the active team's avatar + name. No Devices tab (a Team row), no
// Inbox tab (every page header), no More, no Actions.
describe(`MobileTabBar tabs (chats.json shell.tabs)`, () => {
  beforeEach(() => {
    route.value = ``
    memberTeams.value = null
    perms.canCreate = true
    chatsNav.value = { count: 0, tone: `muted` }
  })

  it(`draws Board · Chats · Apps · Team and nothing else`, () => {
    renderBar()
    const nav = screen.getByRole(`navigation`, { name: `Primary` })
    const tabs = [...nav.querySelectorAll(`a, button`)]
    expect(tabs.map((el) => el.getAttribute(`aria-label`))).toEqual([
      `Web`,
      `Chats`,
      `Apps`,
      `Acme`,
    ])
    expect(tabs.map((el) => el.textContent)).toEqual([`Web`, `Chats`, `Apps`, `Acme`]) // the avatar initial, then the name
    expect(screen.getByTestId(`tab-team`).getAttribute(`href`)).toBe(`/t/$teamSlug/team`)
    expect(screen.getByTestId(`tab-apps`).getAttribute(`href`)).toBe(`/t/$teamSlug/apps`)
    expect(screen.queryByTestId(`tab-devices`)).toBeNull()
    expect(screen.queryByLabelText(`Inbox`)).toBeNull()
    expect(screen.queryByTestId(`nav-more`)).toBeNull()
    expect(screen.queryByLabelText(`Settings`)).toBeNull()
  })

  it(`labels the Board tab Board while the team has no board`, () => {
    renderBar([])
    expect(screen.getByTestId(`tab-board`).getAttribute(`aria-label`)).toBe(`Board`)
  })

  it(`lights the Team tab on its own route, the settings and Devices it pushes`, () => {
    for (const path of [`/t/$teamSlug/team`, `/t/$teamSlug/settings`, `/t/$teamSlug/devices`]) {
      route.value = path
      const view = renderBar()
      expect(screen.getByTestId(`tab-team`).getAttribute(`aria-current`), path).toBe(`page`)
      view.unmount()
    }
  })

  it(`lights the Apps tab on the Apps list`, () => {
    route.value = `/t/$teamSlug/apps`
    renderBar()
    expect(screen.getByTestId(`tab-apps`).getAttribute(`aria-current`)).toBe(`page`)
    expect(screen.getByTestId(`tab-team`).getAttribute(`aria-current`)).toBeNull()
  })

  it(`links the Board tab to the board off the board view, no caret`, () => {
    route.value = `/t/$teamSlug/chats`
    renderBar()
    const tab = screen.getByTestId(`tab-board`)
    expect(tab.tagName).toBe(`A`)
    expect(tab.getAttribute(`aria-haspopup`)).toBeNull()
  })

  it(`opens the board menu from the Board tab on the board view`, () => {
    route.value = `/t/$teamSlug/boards/$boardSlug`
    navigate.mockReset()
    renderBar()
    const tab = screen.getByTestId(`tab-board`)
    expect(tab.tagName).toBe(`BUTTON`)
    expect(tab.getAttribute(`aria-haspopup`)).toBe(`menu`)
    expect(screen.queryByTestId(`board-menu`)).toBeNull()
    fireEvent.click(tab)
    expect(screen.getByTestId(`board-menu`)).toBeTruthy()
    const rows = screen.getAllByTestId(`board-menu-row`)
    expect(rows.map((row) => row.textContent)).toEqual([`Web`, `iOS`, `New board`])
    // No board in the params: the fallback (first) board is the active one.
    expect(rows[0].getAttribute(`aria-checked`)).toBe(`true`)
    fireEvent.click(rows[1])
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug: `acme`, boardSlug: `ios` },
    })
  })

  it(`keeps the caret out of the label row, so a tab's width never changes`, () => {
    route.value = `/t/$teamSlug/boards/$boardSlug`
    renderBar()
    const tab = screen.getByTestId(`tab-board`)
    const caret = tab.querySelector(`[data-slot=phone-tab-caret]`) as HTMLElement
    expect(caret.className).toContain(`absolute`)
    expect(screen.getByTestId(`tab-team`).querySelector(`[data-slot=phone-tab-caret]`)).toBeNull()
  })

  it(`opens the team menu from the Team tab on the Team page only`, () => {
    route.value = `/t/$teamSlug/team`
    navigate.mockReset()
    renderBar()
    const tab = screen.getByTestId(`tab-team`)
    expect(tab.getAttribute(`aria-haspopup`)).toBe(`menu`)
    expect(tab.querySelector(`[data-slot=phone-tab-caret]`)).not.toBeNull()
    fireEvent.click(tab)
    const rows = screen.getAllByTestId(`board-menu-row`)
    expect(rows.map((row) => row.textContent)).toEqual([`Acme`, `Globex`, `New team`])
    expect(rows[0].getAttribute(`aria-checked`)).toBe(`true`)
  })

  it(`offers New board only to a viewer who may create boards`, () => {
    route.value = `/t/$teamSlug/boards/$boardSlug`
    perms.canCreate = false
    renderBar()
    fireEvent.click(screen.getByTestId(`tab-board`))
    expect(screen.getAllByTestId(`board-menu-row`).map((row) => row.textContent)).toEqual([
      `Web`,
      `iOS`,
    ])
  })

  it(`hides on the pushed Inbox`, () => {
    route.value = `/t/$teamSlug/inbox`
    renderBar()
    expect(screen.queryByRole(`navigation`, { name: `Primary` })).toBeNull()
  })

  it(`selects the chat arm on the Agent page only`, () => {
    route.value = `/t/$teamSlug/agent`
    const view = renderBar()
    const chat = screen.getByTestId(`chat-button`)
    expect(chat.getAttribute(`aria-current`)).toBe(`page`)
    expect(chat.innerHTML).toContain(`bg-glass-active`)
    view.unmount()
    route.value = `/t/$teamSlug/devices`
    renderBar()
    expect(screen.getByTestId(`chat-button`).getAttribute(`aria-current`)).toBeNull()
    expect(screen.getByTestId(`chat-button`).innerHTML).not.toContain(
      `bg-glass-active`
    )
  })

  // NEW-2: prompts replaced actions; the phone has no Actions tab.
  it(`draws no Actions tab`, () => {
    renderBar()
    expect(screen.queryByTestId(`tab-actions`)).toBeNull()
    expect(screen.queryByLabelText(`Actions`)).toBeNull()
  })
})
