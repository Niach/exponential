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

import {
  useSecondSidebar,
  useSidebarOccupant,
} from "@/hooks/use-sidebar-occupant"
afterEach(() => {
  location.value = { pathname: `/t/acme/agent`, search: {} }
})

describe(`useSidebarOccupant`, () => {
  it(`is the main menu on the Agent page, Threads and a thread`, () => {
    for (const pathname of [
      `/t/acme/agent`,
      `/t/acme/threads`,
      `/t/acme/threads/t1`,
      `/t/acme/agents/a1`,
      `/t/acme/sessions/s1`,
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

describe(`useSecondSidebar`, () => {
  // EXP-1246: the diff face no longer swaps the panel — the inbox stays.
  it(`keeps the inbox beside an issue on any face`, () => {
    location.value = {
      pathname: `/t/acme/boards/web/issues/MET-1`,
      search: { view: `diff`, from: `inbox` },
    }
    expect(renderHook(() => useSecondSidebar()).result.current).toEqual({
      kind: `inbox`,
    })
  })

  it(`reads the inbox page's tab`, () => {
    location.value = { pathname: `/t/acme/inbox`, search: { tab: `my-issues` } }
    expect(renderHook(() => useSecondSidebar()).result.current).toEqual({
      kind: `inbox`,
      tab: `my-issues`,
    })
  })

  it(`shows none for a running-origin detail`, () => {
    location.value = {
      pathname: `/t/acme/sessions/s1`,
      search: { from: `running` },
    }
    expect(renderHook(() => useSecondSidebar()).result.current).toBeNull()
  })
})
