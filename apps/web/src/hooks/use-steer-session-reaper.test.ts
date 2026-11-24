import { renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// The reaper lives in the team layout (a phone's sidebar never mounts).
// Pinned: the keep set is my non-ended sessions plus the route's session, it
// runs with no team resolved yet, and it re-runs on changes.

const mockState = vi.hoisted(() => ({
  rows: [] as { id: string; status: string; teamId?: string }[],
  teams: [{ id: `team-1` }] as { id: string }[],
  params: {} as Record<string, string | undefined>,
  retain: vi.fn(),
  queried: false,
}))

vi.mock(`@tanstack/react-router`, () => ({
  useParams: () => mockState.params,
}))

vi.mock(`@tanstack/react-db`, () => ({
  and: () => null,
  eq: () => null,
  useLiveQuery: (build: (query: unknown) => unknown) => {
    const chain: Record<string, unknown> = {}
    let teams = false
    chain.from = (source: Record<string, unknown>) => {
      teams = `teams` in source
      return chain
    }
    for (const key of [`where`, `select`]) chain[key] = () => chain
    const built = build(chain) !== undefined
    if (teams) return { data: mockState.teams }
    mockState.queried = built
    return { data: built ? mockState.rows.map((row) => ({ teamId: `team-1`, ...row })) : undefined }
  },
}))

vi.mock(`@/lib/collections`, () => ({ sessionCollection: {}, teamCollection: {} }))

vi.mock(`@/lib/steer-session-store`, () => ({
  retainSteerSessions: mockState.retain,
}))

import { useSteerSessionReaper } from "@/hooks/use-steer-session-reaper"

const lastKeep = () =>
  [...(mockState.retain.mock.calls.at(-1)![0] as Set<string>)].sort()

describe(`useSteerSessionReaper`, () => {
  beforeEach(() => {
    mockState.rows = []
    mockState.params = {}
    mockState.retain.mockClear()
  })

  it(`retains my live stores plus the one the route is showing`, () => {
    mockState.rows = [
      { id: `s1`, status: `running` },
      { id: `s2`, status: `queued` },
      { id: `s3`, status: `idle` },
      { id: `gone`, status: `ended` },
    ]
    mockState.params = { teamSlug: `acme`, sessionId: `s-ended` }
    renderHook(() => useSteerSessionReaper(`team-1`, `user-1`))
    expect(lastKeep()).toEqual([`s-ended`, `s1`, `s2`, `s3`])
  })

  it(`runs before the team resolves, keeping only the route's session`, () => {
    mockState.rows = [{ id: `s1`, status: `running` }]
    mockState.params = { teamSlug: `acme`, sessionId: `s9` }
    renderHook(() => useSteerSessionReaper(undefined, undefined))
    expect(mockState.queried).toBe(false)
    expect(lastKeep()).toEqual([`s9`])
  })

  it(`re-retains when the live set or the route changes`, () => {
    mockState.rows = [{ id: `s1`, status: `running` }]
    const { rerender } = renderHook(() => useSteerSessionReaper(`team-1`, `user-1`))
    expect(lastKeep()).toEqual([`s1`])
    mockState.rows = [
      { id: `s1`, status: `running` },
      { id: `s2`, status: `running` },
    ]
    rerender()
    expect(lastKeep()).toEqual([`s1`, `s2`])
    mockState.params = { teamSlug: `acme`, sessionId: `s3` }
    rerender()
    expect(lastKeep()).toEqual([`s1`, `s2`, `s3`])
  })

  // #EXP-2079 (agent-settings.json `assistant.chatTeams` LIVE STATE): my own
  // session in a team I do not sync (the Assistant's conversation) is kept
  // on every team's page; another synced team's session is not.
  it(`keeps my session of a team that is not synced, whatever team's page is open`, () => {
    mockState.rows = [
      { id: `s1`, status: `running` },
      { id: `assistant`, status: `idle`, teamId: `personal` },
      { id: `other`, status: `running`, teamId: `team-2` },
      { id: `gone`, status: `ended`, teamId: `personal` },
    ]
    mockState.teams = [{ id: `team-1` }, { id: `team-2` }]
    renderHook(() => useSteerSessionReaper(`team-1`, `user-1`))
    expect(lastKeep()).toEqual([`assistant`, `s1`])
  })
})
