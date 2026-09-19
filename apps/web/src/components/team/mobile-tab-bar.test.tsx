import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Board, Team } from "@/db/schema"

// EXP-973: the phone's FAB is the SAME split capsule on EVERY tab-bar route.
// Devices, Actions/Automations, Reviews, Inbox and Support used to drop the
// New-issue arm and leave a lone chat circle, which turned "file this" into a
// trip back to a board. The only thing that still collapses the capsule is
// having NO board to file into.

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
vi.mock(`@/hooks/use-nav-counts`, () => ({
  useReviewsOpenPrCount: () => 0,
  useAgentsRunningCount: () => ({ count: 0, needsInput: false }),
}))

import { MobileTabBar } from "@/components/team/mobile-tab-bar"

const team = { id: `t1`, name: `Acme`, helpdeskEnabled: false } as Team
const boards = [{ id: `b1`, slug: `web`, name: `Web` }] as Board[]

function renderBar(boardList: Board[] | undefined = boards) {
  return render(
    <MobileTabBar teamSlug="acme" team={team} boards={boardList} />
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

  it(`collapses to the chat circle only when there is no board at all`, () => {
    route.value = `/t/$teamSlug/devices`
    renderBar([])
    expect(screen.queryByTestId(`fab-group`)).toBeNull()
    expect(screen.queryByTestId(`compose-button`)).toBeNull()
    expect(screen.getByTestId(`chat-button`)).toBeTruthy()
  })
})
