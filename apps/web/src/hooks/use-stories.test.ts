import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ agentCollection: {}, messageCollection: {}, sessionReadsCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/components/thread/agent-channel`, () => ({ useAnswerMessage: () => ({}) }))
vi.mock(`@/hooks/use-session-tree`, () => ({}))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))

const { backgroundStoryRoots } = await import(`./use-stories`)

const NOW = Date.parse(`2026-10-28T12:00:00Z`)
const DAY = 86_400_000
const root = (id: string, ageDays: number, over: { agentId?: string | null; userId?: string } = {}) => ({
  id,
  rootId: id,
  agentId: over.agentId ?? null,
  userId: over.userId ?? `me`,
  createdAt: new Date(NOW - ageDays * DAY),
  updatedAt: new Date(NOW - ageDays * DAY),
})

describe(`backgroundStoryRoots`, () => {
  it(`takes my agent conversations whatever their age, then every root whose tree moved within the keep window, uncapped`, () => {
    const rows = [
      root(`conv`, 30, { agentId: `a1` }),
      root(`theirs`, 0, { agentId: `a1`, userId: `you` }),
      root(`old`, 8),
      root(`r1`, 1),
      root(`r2`, 2),
      root(`r3`, 3),
      { ...root(`r1`, 1), id: `child`, rootId: `r1` },
      // A stale root whose child posted this week (F40).
      root(`quiet`, 20),
      { ...root(`quiet`, 2), id: `quiet-child`, rootId: `quiet` },
    ]
    expect(backgroundStoryRoots(rows, `me`, NOW)).toEqual([`conv`, `quiet`, `r1`, `r2`, `r3`, `theirs`])
    const many = Array.from({ length: 40 }, (_, i) => root(`n${String(i).padStart(2, `0`)}`, 1))
    expect(backgroundStoryRoots(many, `me`, NOW)).toHaveLength(40)
  })
})
