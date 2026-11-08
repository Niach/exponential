import { renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

// EXP-923 / EXP-1246 / #EXP-1672: the sidebar occupant and the card's second
// sidebar off the router state alone.

const location = vi.hoisted(() => ({
  value: { pathname: `/t/acme/agent`, search: {} as Record<string, unknown> },
}))

vi.mock(`@tanstack/react-router`, () => ({
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location: location.value }),
}))

import { useSidebarOccupant } from "@/hooks/use-sidebar-occupant"
afterEach(() => {
  location.value = { pathname: `/t/acme/agent`, search: {} }
})

describe(`useSidebarOccupant`, () => {
  it(`is the main menu everywhere but Settings: nothing folds`, () => {
    for (const pathname of [
      `/t/acme/agent`,
      `/t/acme/chats`,
      `/t/acme/chats/t1`,
      `/t/acme/agents/a1`,
      `/t/acme/sessions/s1`,
      `/t/acme/inbox`,
      `/t/acme/devices`,
      `/t/acme/apps`,
      `/t/acme/apps/working-hours`,
      `/t/acme/boards/app`,
      `/t/acme/boards/app/issues/APP-1`,
      `/t/acme/drafts`,
    ]) {
      location.value = { pathname, search: { from: `agent:recent` } }
      const { result } = renderHook(() => useSidebarOccupant())
      expect(result.current, pathname).toEqual({ kind: `main` })
    }
  })

  it(`yields to settings`, () => {
    location.value = { pathname: `/t/acme/settings/general`, search: {} }
    expect(renderHook(() => useSidebarOccupant()).result.current).toEqual({
      kind: `settings`,
    })
  })

  // #EXP-1672: the Inbox never folds the main menu — its list is the card's
  // second sidebar.
  it(`keeps the main menu on the inbox and beside an inbox detail`, () => {
    for (const [pathname, search] of [
      [`/t/acme/inbox`, {}],
      [`/t/acme/boards/web/issues/MET-1`, { from: `inbox` }],
    ] as const) {
      location.value = { pathname, search }
      expect(renderHook(() => useSidebarOccupant()).result.current).toEqual({
        kind: `main`,
      })
    }
  })
})
