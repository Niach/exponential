import { renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// The reaper left the sidebar's Sessions group (never mounted on phones) for
// the team layout. Pinned: the keep set is the running rows plus the route's
// session, it runs with no team resolved yet, and it re-runs on changes.

const mockState = vi.hoisted(() => ({
  running: [] as { session: { id: string } }[],
  params: {} as Record<string, string | undefined>,
  retain: vi.fn(),
  agentsArgs: [] as unknown[],
}))

vi.mock(`@tanstack/react-router`, () => ({
  useParams: () => mockState.params,
}))

vi.mock(`@/hooks/use-agents-data`, () => ({
  useAgentsData: (...args: unknown[]) => {
    mockState.agentsArgs = args
    return { running: mockState.running, isLoading: false }
  },
}))

vi.mock(`@/lib/steer-session-store`, () => ({
  retainSteerSessions: mockState.retain,
}))

import { useSteerSessionReaper } from "@/hooks/use-steer-session-reaper"

const row = (id: string) => ({ session: { id } })

const lastKeep = () =>
  [...(mockState.retain.mock.calls.at(-1)![0] as Set<string>)].sort()

describe(`useSteerSessionReaper`, () => {
  beforeEach(() => {
    mockState.running = []
    mockState.params = {}
    mockState.retain.mockClear()
  })

  it(`retains the running stores plus the one the route is showing`, () => {
    mockState.running = [row(`s1`), row(`s2`)]
    mockState.params = { teamSlug: `acme`, sessionId: `s-ended` }
    renderHook(() => useSteerSessionReaper(`team-1`, `user-1`))
    expect(mockState.agentsArgs).toEqual([`team-1`, `user-1`])
    expect(lastKeep()).toEqual([`s-ended`, `s1`, `s2`])
  })

  it(`runs before the team resolves, keeping only the route's session`, () => {
    mockState.params = { teamSlug: `acme`, sessionId: `s9` }
    renderHook(() => useSteerSessionReaper(undefined, undefined))
    expect(mockState.retain).toHaveBeenCalledTimes(1)
    expect(lastKeep()).toEqual([`s9`])
  })

  it(`re-retains when the running set or the route changes`, () => {
    mockState.running = [row(`s1`)]
    const { rerender } = renderHook(() =>
      useSteerSessionReaper(`team-1`, `user-1`)
    )
    expect(lastKeep()).toEqual([`s1`])
    mockState.running = [row(`s1`), row(`s2`)]
    rerender()
    expect(lastKeep()).toEqual([`s1`, `s2`])
    mockState.params = { teamSlug: `acme`, sessionId: `s3` }
    rerender()
    expect(lastKeep()).toEqual([`s1`, `s2`, `s3`])
  })
})
