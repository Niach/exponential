import { beforeEach, describe, expect, it, vi } from "vitest"

// REV2-7: getUserTeamIds / getReadableUserIdsInTeams are re-resolved by
// every Electric shape long-poll renewal (12 + 1 of the 14 shapes), so
// membership.ts routes them through the short-TTL caches in
// membership-cache.ts. These tests pin the integration: concurrent callers
// share one query, invalidation forces a re-query, and DB errors are never
// cached. (TTL/expiry/bound semantics are pinned in ttl-promise-cache.test.ts.)

const h = vi.hoisted(() => {
  const state = {
    selectCount: 0,
    queue: [] as unknown[][],
    failNext: false,
  }
  return { state }
})

vi.mock(`@/db/connection`, () => {
  function selectChain() {
    h.state.selectCount++
    const p = (
      h.state.failNext
        ? Promise.reject(new Error(`db down`))
        : Promise.resolve(h.state.queue.shift() ?? [])
    ) as Promise<unknown[]> & Record<string, () => unknown>
    h.state.failNext = false
    for (const m of [`from`, `where`]) {
      p[m] = () => p
    }
    return p
  }
  return { db: { select: () => selectChain() } }
})

import {
  getReadableUserIdsInTeams,
  getUserTeamIds,
} from "@/lib/auth/membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"

beforeEach(() => {
  h.state.selectCount = 0
  h.state.queue.length = 0
  h.state.failNext = false
  invalidateMembershipCaches()
})

describe(`getUserTeamIds caching (REV2-7)`, () => {
  it(`coalesces concurrent callers into one query`, async () => {
    h.state.queue.push([{ teamId: `t1` }, { teamId: `t2` }])

    const [a, b, c] = await Promise.all([
      getUserTeamIds(`u1`),
      getUserTeamIds(`u1`),
      getUserTeamIds(`u1`),
    ])

    expect(h.state.selectCount).toBe(1)
    expect(a).toEqual([`t1`, `t2`])
    expect(b).toEqual(a)
    expect(c).toEqual(a)
  })

  it(`serves sequential calls within the TTL from cache`, async () => {
    h.state.queue.push([{ teamId: `t1` }])
    await getUserTeamIds(`u1`)
    await getUserTeamIds(`u1`)
    expect(h.state.selectCount).toBe(1)
  })

  it(`caches per user`, async () => {
    h.state.queue.push([{ teamId: `t1` }], [{ teamId: `t2` }])
    expect(await getUserTeamIds(`u1`)).toEqual([`t1`])
    expect(await getUserTeamIds(`u2`)).toEqual([`t2`])
    expect(h.state.selectCount).toBe(2)
  })

  it(`invalidateMembershipCaches forces a re-query`, async () => {
    h.state.queue.push([{ teamId: `t1` }], [{ teamId: `t1` }, { teamId: `t2` }])
    expect(await getUserTeamIds(`u1`)).toEqual([`t1`])

    invalidateMembershipCaches()

    expect(await getUserTeamIds(`u1`)).toEqual([`t1`, `t2`])
    expect(h.state.selectCount).toBe(2)
  })

  it(`does not cache a failed query — the next call retries`, async () => {
    h.state.failNext = true
    await expect(getUserTeamIds(`u1`)).rejects.toThrow(`db down`)

    h.state.queue.push([{ teamId: `t1` }])
    expect(await getUserTeamIds(`u1`)).toEqual([`t1`])
    expect(h.state.selectCount).toBe(2)
  })
})

describe(`getReadableUserIdsInTeams caching (REV2-7)`, () => {
  it(`coalesces concurrent callers into one two-query pass`, async () => {
    h.state.queue.push(
      [{ teamId: `t1` }],
      [{ userId: `u1` }, { userId: `u2` }, { userId: `u2` }]
    )

    const [a, b] = await Promise.all([
      getReadableUserIdsInTeams(`u1`),
      getReadableUserIdsInTeams(`u1`),
    ])

    expect(h.state.selectCount).toBe(2)
    expect(a).toEqual([`u1`, `u2`])
    expect(b).toEqual(a)
  })

  it(`caches the membership-less self-only result too`, async () => {
    h.state.queue.push([])
    expect(await getReadableUserIdsInTeams(`u1`)).toEqual([`u1`])
    expect(await getReadableUserIdsInTeams(`u1`)).toEqual([`u1`])
    expect(h.state.selectCount).toBe(1)
  })

  it(`anonymous callers bypass the cache entirely`, async () => {
    expect(await getReadableUserIdsInTeams(null)).toEqual([])
    expect(h.state.selectCount).toBe(0)
  })
})
