import { renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

// EXP-923 / EXP-1246: the sidebar occupant off the router state alone.

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

  // EXP-1246: the diff face no longer swaps the panel — the inbox stays.
  it(`keeps the inbox beside an issue on any face`, () => {
    location.value = {
      pathname: `/t/acme/boards/web/issues/MET-1`,
      search: { view: `diff`, from: `inbox` },
    }
    expect(renderHook(() => useSidebarOccupant()).result.current).toEqual({
      kind: `list`,
      origin: { kind: `inbox` },
    })
  })

  it(`reads the inbox page's tab`, () => {
    location.value = { pathname: `/t/acme/inbox`, search: { tab: `my-issues` } }
    expect(renderHook(() => useSidebarOccupant()).result.current).toEqual({
      kind: `list`,
      origin: { kind: `inbox`, tab: `my-issues` },
    })
  })

  it(`keeps the main menu for a running-origin detail`, () => {
    location.value = {
      pathname: `/t/acme/sessions/s1`,
      search: { from: `running` },
    }
    expect(renderHook(() => useSidebarOccupant()).result.current).toEqual({
      kind: `main`,
    })
  })
})
