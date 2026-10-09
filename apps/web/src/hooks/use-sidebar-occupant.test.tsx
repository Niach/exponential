import { act, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

// EXP-923 / EXP-1246: the sidebar occupant off the router state. The Recent
// panel flag is the one input the URL does not decide, and it only counts on
// the Agent page; a run opened from Recent carries `?from=agent:recent`.

const location = vi.hoisted(() => ({
  value: { pathname: `/t/acme/agent`, search: {} as Record<string, unknown> },
}))

vi.mock(`@tanstack/react-router`, () => ({
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location: location.value }),
}))

import { useSidebarOccupant } from "@/hooks/use-sidebar-occupant"
import {
  setRecentRunsPanelOpen,
  toggleRecentRunsPanel,
} from "@/lib/recent-runs-panel"

const recent = { kind: `list`, origin: { kind: `agent`, tab: `recent` } }

afterEach(() => {
  setRecentRunsPanelOpen(false)
  location.value = { pathname: `/t/acme/agent`, search: {} }
})

describe(`useSidebarOccupant`, () => {
  it(`is the main menu on the Agent page until the toggle is on`, () => {
    const { result } = renderHook(() => useSidebarOccupant())
    expect(result.current).toEqual({ kind: `main` })
    act(() => toggleRecentRunsPanel())
    expect(result.current).toEqual(recent)
    act(() => toggleRecentRunsPanel())
    expect(result.current).toEqual({ kind: `main` })
  })

  it(`never takes the flag to another route`, () => {
    setRecentRunsPanelOpen(true)
    for (const pathname of [
      `/t/acme`,
      `/t/acme/sessions/s1`,
      `/t/acme/boards/web/issues/MET-1`,
      `/t/acme/boards/web`,
    ]) {
      location.value = { pathname, search: {} }
      const { result } = renderHook(() => useSidebarOccupant())
      expect(result.current, pathname).toEqual({ kind: `main` })
    }
  })

  it(`keeps Recent beside a run opened from it`, () => {
    location.value = {
      pathname: `/t/acme/sessions/s1`,
      search: { from: `agent:recent` },
    }
    expect(renderHook(() => useSidebarOccupant()).result.current).toEqual(
      recent
    )
  })

  it(`yields to settings`, () => {
    setRecentRunsPanelOpen(true)
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
