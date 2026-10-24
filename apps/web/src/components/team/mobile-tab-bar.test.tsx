import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Board, Team } from "@/db/schema"

// EXP-973: the phone's FAB is the SAME split capsule on EVERY tab-bar route.
// Devices, Actions/Automations, Threads, Inbox and Support used to drop the
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
const threadsNav = vi.hoisted(() => ({
  value: { live: false, needsYou: false, unread: false },
  scope: undefined as string | readonly string[] | undefined,
}))
vi.mock(`@/hooks/use-nav-counts`, () => ({
  useThreadsNav: (teamIds: string | readonly string[] | undefined) => {
    threadsNav.scope = teamIds
    return threadsNav.value
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
}))

import { MobileTabBar } from "@/components/team/mobile-tab-bar"

const team = { id: `t1`, name: `Acme` } as Team
const boards = [{ id: `b1`, slug: `web`, name: `Web` }] as Board[]

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
      `/t/$teamSlug/threads`,
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
    route.value = `/t/$teamSlug/threads`
    renderBar()
    const compose = screen.getByTestId(`compose-button`)
    expect(compose.getAttribute(`aria-disabled`)).toBeNull()
    expect(compose.getAttribute(`aria-label`)).toBe(`New issue`)
  })
})

// EXP-1293 → thread 58cb32df: Chats — always a tab, its dot amber while a
// chat needs you, green while one runs, else the unread dot; phones read
// the ONE active team (chats.json `shell`).
describe(`MobileTabBar Chats (chats.json)`, () => {
  beforeEach(() => {
    route.value = ``
    memberTeams.value = null
    threadsNav.value = { live: false, needsYou: false, unread: false }
  })

  const chatsTab = () => screen.getByTestId(`tab-threads`)

  it(`links Chats and lights it on its routes`, () => {
    route.value = `/t/$teamSlug/threads`
    renderBar()
    expect(chatsTab().getAttribute(`href`)).toBe(`/t/$teamSlug/threads`)
    expect(chatsTab().className).toContain(`bg-glass-active`)
    expect(chatsTab().getAttribute(`aria-current`)).toBe(`page`)
  })

  it(`dots Chats amber while it needs you, green while running, else unread`, () => {
    const view = renderBar()
    expect(chatsTab().querySelector(`[data-slot=phone-tab-dot]`)).toBeNull()
    view.unmount()
    threadsNav.value = { live: true, needsYou: false, unread: false }
    const running = renderBar()
    expect(chatsTab().querySelector(`.bg-green-500`)).toBeTruthy()
    running.unmount()
    threadsNav.value = { live: true, needsYou: true, unread: false }
    const needs = renderBar()
    expect(chatsTab().querySelector(`.bg-yellow-400`)).toBeTruthy()
    needs.unmount()
    threadsNav.value = { live: false, needsYou: false, unread: true }
    renderBar()
    expect(chatsTab().querySelector(`.bg-primary`)).toBeTruthy()
  })

  it(`reads the dot off the active team only`, () => {
    memberTeams.value = [team, { id: `t2`, name: `Beta` }]
    renderBar()
    expect(threadsNav.scope).toBe(`t1`)
  })

  // EXP-1327: the live dot rides Chats, never the chat arm.
  it(`never dots the chat arm`, () => {
    threadsNav.value = { live: true, needsYou: true, unread: false }
    renderBar()
    expect(
      screen.getByTestId(`chat-button`).querySelector(`.bg-green-500, .bg-yellow-400`)
    ).toBeNull()
  })
})

// chats.json `shell.tabs`: Board · Devices · Chats · Team, each an icon over
// its label; the Team tab = the active team's avatar + name. No Inbox tab
// (it moved into the Chats header), no More, no Actions.
describe(`MobileTabBar tabs (chats.json shell.tabs)`, () => {
  beforeEach(() => {
    route.value = ``
    memberTeams.value = null
  })

  it(`draws Board · Devices · Chats · Team and nothing else`, () => {
    renderBar()
    const nav = screen.getByRole(`navigation`, { name: `Primary` })
    const tabs = [...nav.querySelectorAll(`a, button`)]
    expect(tabs.map((el) => el.getAttribute(`aria-label`))).toEqual([
      `Board`,
      `Devices`,
      `Chats`,
      `Acme`,
    ])
    expect(tabs.map((el) => el.textContent)).toEqual([`Board`, `Devices`, `Chats`, `AAcme`]) // the avatar initial, then the name
    expect(screen.getByTestId(`tab-team`).getAttribute(`href`)).toBe(`/t/$teamSlug/team`)
    expect(screen.queryByLabelText(`Inbox`)).toBeNull()
    expect(screen.queryByTestId(`nav-more`)).toBeNull()
    expect(screen.queryByLabelText(`Settings`)).toBeNull()
  })

  it(`lights the Team tab on its own route and the settings it pushes`, () => {
    for (const path of [`/t/$teamSlug/team`, `/t/$teamSlug/settings`]) {
      route.value = path
      const view = renderBar()
      expect(screen.getByTestId(`tab-team`).getAttribute(`aria-current`), path).toBe(`page`)
      view.unmount()
    }
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
