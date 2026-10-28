import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1418: a fake pool (the advisory lock's connection) and a db whose
// root read resolves when the test says so (no yolo team: `not_yolo`).
const h = vi.hoisted(() => {
  const state = {
    reads: 0,
    gate: null as Promise<void> | null,
    unlockFails: false,
    locked: true,
    released: [] as unknown[],
  }
  const client = {
    query: vi.fn(async (text: string) => {
      if (text.includes(`pg_advisory_unlock`)) {
        if (state.unlockFails) throw new Error(`connection reset`)
        return { rows: [{ unlocked: true }] }
      }
      return { rows: [{ locked: state.locked }] }
    }),
    release: vi.fn((err?: unknown) => {
      state.released.push(err)
    }),
  }
  const chain = (): unknown => {
    const p = (async (): Promise<unknown[]> => {
      state.reads += 1
      if (state.gate) await state.gate
      return []
    })() as unknown as Promise<unknown[]> & Record<string, () => unknown>
    for (const m of [`from`, `innerJoin`, `where`, `limit`]) p[m] = () => p
    return p
  }
  return { state, client, db: { select: () => chain() }, pool: { connect: async () => client } }
})

vi.mock(`@/db/connection`, () => ({ db: h.db, pool: h.pool }))
vi.mock(`@/lib/integrations/pr-sync`, () => ({ livePullRequestsOfRoot: vi.fn() }))
vi.mock(`@/lib/pull-requests`, () => ({ mergePullRequestOrThrow: vi.fn() }))

import { maybeMergeYoloThread, yoloMergeOrder } from "@/lib/yolo-tree-merge"

// EXP-1291: a thread's PRs merge parents first (base_pull_request_id), so a
// child never squashes into its parent's branch.
describe(`yoloMergeOrder`, () => {
  const at = (minute: number) => new Date(Date.UTC(2026, 9, 13, 12, minute))

  it(`puts every parent before its children, oldest first among peers`, () => {
    const order = yoloMergeOrder([
      { id: `child`, basePullRequestId: `root`, createdAt: at(1) },
      { id: `grandchild`, basePullRequestId: `child`, createdAt: at(0) },
      { id: `root`, basePullRequestId: null, createdAt: at(5) },
      { id: `lone`, basePullRequestId: null, createdAt: at(2) },
    ])
    expect(order).toEqual([`lone`, `root`, `child`, `grandchild`])
  })

  it(`treats a parent outside the set as merged already`, () => {
    expect(
      yoloMergeOrder([{ id: `a`, basePullRequestId: `gone`, createdAt: at(0) }])
    ).toEqual([`a`])
  })

  it(`survives a cycle`, () => {
    expect(
      yoloMergeOrder([
        { id: `a`, basePullRequestId: `b`, createdAt: at(0) },
        { id: `b`, basePullRequestId: `a`, createdAt: at(1) },
      ])
    ).toEqual([`a`, `b`])
  })
})

describe(`#EXP-1418: the per-thread pass`, () => {
  beforeEach(() => {
    h.state.reads = 0
    h.state.gate = null
    h.state.unlockFails = false
    h.state.locked = true
    h.state.released.length = 0
  })

  it(`an unlock that throws discards the connection instead of pooling it with the lock held`, async () => {
    h.state.unlockFails = true
    expect(await maybeMergeYoloThread(`root-a`)).toEqual({ status: `not_yolo` })
    expect(h.state.released).toHaveLength(1)
    expect(h.state.released[0]).toBeInstanceOf(Error)
    // A clean pass returns its connection to the pool.
    h.state.unlockFails = false
    await maybeMergeYoloThread(`root-a`)
    expect(h.state.released[1]).toBeUndefined()
  })

  it(`a trigger during a running pass re-runs it once at its end (coalesced)`, async () => {
    let open!: () => void
    h.state.gate = new Promise<void>((resolve) => (open = resolve))
    const first = maybeMergeYoloThread(`root-b`)
    await vi.waitFor(() => expect(h.state.reads).toBe(1))
    const second = maybeMergeYoloThread(`root-b`)
    const third = maybeMergeYoloThread(`root-b`)
    h.state.gate = null
    open()
    expect(await first).toEqual({ status: `not_yolo` })
    expect(await second).toEqual(await third)
    // Two triggers during the pass = ONE more pass, not two.
    expect(h.state.reads).toBe(2)
  })

  it(`no trigger during the pass = no re-run`, async () => {
    await maybeMergeYoloThread(`root-c`)
    expect(h.state.reads).toBe(1)
  })
})
