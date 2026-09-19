import { act, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

// EXP-923: the Recent runs panel is the ONE sidebar occupant the URL does not
// decide — a disclosure on the Agent page, held in a module store. The rules
// it has to keep: only on that route, only while the toggle is on, and never
// after the page is left.

const location = vi.hoisted(() => ({
  value: { pathname: `/t/acme/agent`, search: {} as Record<string, unknown> },
}))

vi.mock(`@tanstack/react-router`, () => ({
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location: location.value }),
}))
vi.mock(`@/lib/review-files-slot`, () => ({
  useReviewFilesSubjectId: () => null,
}))

import { useSidebarOccupant } from "@/hooks/use-sidebar-occupant"
import {
  setRecentRunsPanelOpen,
  toggleRecentRunsPanel,
} from "@/lib/recent-runs-panel"

afterEach(() => {
  setRecentRunsPanelOpen(false)
  location.value = { pathname: `/t/acme/agent`, search: {} }
})

describe(`useSidebarOccupant (EXP-923)`, () => {
  it(`is the main menu on the Agent page until the toggle is on`, () => {
    const { result } = renderHook(() => useSidebarOccupant())
    expect(result.current).toEqual({ kind: `main` })
    act(() => toggleRecentRunsPanel())
    expect(result.current).toEqual({ kind: `recent` })
    act(() => toggleRecentRunsPanel())
    expect(result.current).toEqual({ kind: `main` })
  })

  it(`never takes the panel to another route`, () => {
    setRecentRunsPanelOpen(true)
    for (const pathname of [
      `/t/acme`,
      `/t/acme/inbox`,
      `/t/acme/sessions/s1`,
      `/t/acme/boards/web/issues/MET-1`,
      `/t/acme/settings`,
    ]) {
      location.value = { pathname, search: {} }
      const { result } = renderHook(() => useSidebarOccupant())
      expect(result.current.kind, pathname).not.toBe(`recent`)
    }
  })

  it(`still yields to settings and to a review's file tree`, () => {
    setRecentRunsPanelOpen(true)
    location.value = { pathname: `/t/acme/settings/general`, search: {} }
    expect(renderHook(() => useSidebarOccupant()).result.current).toEqual({
      kind: `settings`,
    })
    location.value = { pathname: `/t/acme/reviews/MET-1`, search: {} }
    expect(renderHook(() => useSidebarOccupant()).result.current).toEqual({
      kind: `review`,
    })
  })

  // EXP-923: a run opened from the sidebar's Running section keeps the main
  // menu — the section it came from IS in the main menu.
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
