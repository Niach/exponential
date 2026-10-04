import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Board, Team } from "@/db/schema"

// EXP-973: the phone's FAB is the SAME split capsule on EVERY tab-bar route.
// Devices, Actions/Automations, Reviews, Inbox and Support used to drop the
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
vi.mock(`@/hooks/use-unread-notifications`, () => ({
  useUnreadNotificationCount: () => 0,
}))
const openPrs = { current: 0 }
vi.mock(`@/hooks/use-nav-counts`, () => ({
  useReviewsOpenPrCount: () => openPrs.current,
  // Mirrors the real hook: yolo mode hides Reviews unless a PR is open in
  // ANY member team (EXP-1186).
  useShowsReviewsAcrossTeams: (teams: Team[]) =>
    teams.some((t) => t.yoloMode !== true) || openPrs.current > 0,
  useAgentsRunningCount: () => ({ count: 0, needsInput: false }),
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
      `/t/$teamSlug/boards/$boardSlug`,
      `/t/$teamSlug/inbox`,
      `/t/$teamSlug/devices`,
      `/t/$teamSlug/actions`,
      `/t/$teamSlug/reviews`,
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
    route.value = `/t/$teamSlug/reviews`
    renderBar()
    const compose = screen.getByTestId(`compose-button`)
    expect(compose.getAttribute(`aria-disabled`)).toBeNull()
    expect(compose.getAttribute(`aria-label`)).toBe(`New issue`)
  })
})

describe(`MobileTabBar Reviews in yolo mode (EXP-1105)`, () => {
  beforeEach(() => {
    route.value = ``
    openPrs.current = 0
    memberTeams.value = null
  })

  const reviewsTab = () => screen.queryByLabelText(`Reviews`)

  it(`keeps Reviews when yolo mode is off`, () => {
    renderBar()
    expect(reviewsTab()).toBeTruthy()
  })

  it(`hides Reviews in yolo mode while nothing is left open`, () => {
    renderBar(boards, { ...team, yoloMode: true })
    expect(reviewsTab()).toBeNull()
  })

  it(`brings Reviews back in yolo mode while a PR is open`, () => {
    openPrs.current = 1
    renderBar(boards, { ...team, yoloMode: true })
    expect(reviewsTab()).toBeTruthy()
  })

  // EXP-1186: the phone's Reviews is cross-team — one non-yolo member team
  // keeps the tab even while the active one is in yolo mode.
  it(`keeps Reviews while another member team is not in yolo mode`, () => {
    const yolo = { ...team, yoloMode: true }
    memberTeams.value = [yolo, { id: `t2`, name: `Beta`, yoloMode: false }]
    renderBar(boards, yolo)
    expect(reviewsTab()).toBeTruthy()
  })
})

// EXP-1187: no More on the phone — Actions is a tab of its own, Settings
// lives in the topbar's avatar menu (×3 with iOS and Android).
describe(`MobileTabBar tabs (EXP-1187)`, () => {
  beforeEach(() => {
    route.value = ``
    openPrs.current = 0
    memberTeams.value = null
  })

  it(`draws Issues · Inbox · Devices · Reviews · Actions and no More`, () => {
    renderBar()
    const nav = screen.getByRole(`navigation`, { name: `Primary` })
    const labels = [...nav.querySelectorAll(`a, button`)].map((el) =>
      el.getAttribute(`aria-label`)
    )
    expect(labels).toEqual([`Issues`, `Inbox`, `Devices`, `Reviews`, `Actions`])
    expect(screen.queryByTestId(`nav-more`)).toBeNull()
    expect(screen.queryByLabelText(`More`)).toBeNull()
    expect(screen.queryByLabelText(`Settings`)).toBeNull()
  })

  it(`links the Actions tab to the team's actions`, () => {
    renderBar()
    const tab = screen.getByTestId(`tab-actions`)
    expect(tab.getAttribute(`href`)).toBe(`/t/$teamSlug/actions`)
    expect(tab.getAttribute(`aria-label`)).toBe(`Actions`)
  })

  it(`lights the Actions tab on the actions routes only`, () => {
    route.value = `/t/$teamSlug/actions`
    const view = renderBar()
    expect(screen.getByTestId(`tab-actions`).className).toContain(
      `bg-glass-active`
    )
    view.unmount()
    route.value = `/t/$teamSlug/devices`
    renderBar()
    expect(screen.getByTestId(`tab-actions`).className).not.toContain(
      `bg-glass-active`
    )
  })
})
