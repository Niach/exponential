import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Board, Team } from "@/db/schema"

// EXP-973: the phone's FAB is the SAME split capsule on EVERY tab-bar route.
// Devices, Actions/Automations, Reviews, Inbox and Support used to drop the
// New-issue arm and leave a lone chat circle, which turned "file this" into a
// trip back to a board. Nothing collapses it any more: a team with NO board
// dims the arm in place (×3 with iOS / Android `composeEnabled`), so the
// control never moves under the thumb.

const route = vi.hoisted(() => ({ value: `` as string }))

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
  useUnreadSupportCount: () => 0,
}))
const openPrs = { current: 0 }
vi.mock(`@/hooks/use-nav-counts`, () => ({
  useReviewsOpenPrCount: () => openPrs.current,
  // Mirrors the real hook: yolo mode hides Reviews unless a PR is open.
  useShowsReviews: (t?: Team) => t?.yoloMode !== true || openPrs.current > 0,
  useAgentsRunningCount: () => ({ count: 0, needsInput: false }),
}))

import { MobileTabBar } from "@/components/team/mobile-tab-bar"

const team = { id: `t1`, name: `Acme`, helpdeskEnabled: false } as Team
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
      `/t/$teamSlug/automations`,
      `/t/$teamSlug/reviews`,
      `/t/$teamSlug/support`,
    ]) {
      route.value = path
      const view = renderBar()
      expect(screen.getByTestId(`fab-group`), path).toBeTruthy()
      expect(screen.getByTestId(`chat-button`), path).toBeTruthy()
      expect(screen.getByTestId(`compose-button`), path).toBeTruthy()
      view.unmount()
    }
  })

  it(`points the New issue arm at the fallback board off a board route`, () => {
    route.value = `/t/$teamSlug/devices`
    renderBar()
    expect(
      screen.getByTestId(`compose-button`).getAttribute(`href`)
    ).toBe(`/t/$teamSlug/boards/$boardSlug`)
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
})
