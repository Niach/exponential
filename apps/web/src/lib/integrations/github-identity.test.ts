import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the EXP-617 GitHub identity contract. The single most important
// assertion here is the rename guard: a numeric id we have never seen must
// resolve to NOBODY even when a row carries that login, because GitHub logins
// are re-registerable and the previous holder's row would otherwise decide
// whether the squatter's notifications get suppressed.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  selectRejects: false,
  insertValues: [] as unknown[],
  conflictSets: [] as unknown[],
  insertRejects: false,
}))

vi.mock(`@/db/connection`, () => {
  const builder = (result: unknown[]) => {
    const b = {
      from: () => b,
      where: () => b,
      limit: async () => {
        if (h.selectRejects) throw new Error(`db down`)
        return result
      },
    }
    return b
  }
  return {
    db: {
      select: vi.fn(() => builder(h.selectQueue.shift() ?? [])),
      insert: vi.fn(() => ({
        values: (v: unknown) => {
          h.insertValues.push(v)
          return {
            onConflictDoUpdate: async (args: { set: unknown }) => {
              h.conflictSets.push(args.set)
              if (h.insertRejects) throw new Error(`db down`)
            },
          }
        },
      })),
    },
  }
})

import { db } from "@/db/connection"
import {
  isBotActor,
  recordGithubIdentity,
  resolveAppUserForGithubActor,
} from "@/lib/integrations/github-identity"

describe(`isBotActor`, () => {
  it(`rejects the App's own identity by type and by login suffix`, () => {
    expect(isBotActor({ login: `exponential[bot]`, type: `Bot` })).toBe(true)
    expect(isBotActor({ login: `exponential[bot]` })).toBe(true)
    expect(isBotActor({ login: `dependabot[bot]` })).toBe(true)
    expect(isBotActor({ login: `niach`, type: `User` })).toBe(false)
  })
})

describe(`resolveAppUserForGithubActor`, () => {
  beforeEach(() => {
    h.selectQueue.length = 0
    h.selectRejects = false
    vi.mocked(db.select).mockClear()
  })

  it(`resolves a mapped numeric id`, async () => {
    h.selectQueue.push([{ userId: `u1` }])
    expect(await resolveAppUserForGithubActor({ id: 42, login: `niach` })).toBe(
      `u1`
    )
  })

  it(`returns null for an unmapped numeric id even when the login matches`, async () => {
    // The ONE case that must never fall back: id 99 is unknown, so this actor
    // is unmapped — full stop. A login fallback here would hand the decision
    // to whoever owned `niach` before the rename.
    h.selectQueue.push([])
    expect(await resolveAppUserForGithubActor({ id: 99, login: `niach` })).toBe(
      null
    )
    // Exactly one query: no second, login-keyed lookup was attempted.
    expect(vi.mocked(db.select)).toHaveBeenCalledTimes(1)
  })

  it(`never queries for a bot`, async () => {
    vi.mocked(db.select).mockClear()
    expect(await resolveAppUserForGithubActor({ id: 1, login: `x[bot]` })).toBe(
      null
    )
    expect(vi.mocked(db.select)).not.toHaveBeenCalled()
  })

  it(`falls back to a login match only when it is unambiguous`, async () => {
    h.selectQueue.push([{ userId: `u1` }])
    expect(await resolveAppUserForGithubActor({ login: `NiaCh` })).toBe(`u1`)

    h.selectQueue.push([{ userId: `u1` }, { userId: `u2` }])
    expect(await resolveAppUserForGithubActor({ login: `niach` })).toBe(null)
  })

  it(`returns null for a missing actor or a query failure`, async () => {
    expect(await resolveAppUserForGithubActor(null)).toBe(null)
    expect(await resolveAppUserForGithubActor({})).toBe(null)

    h.selectRejects = true
    h.selectQueue.push([{ userId: `u1` }])
    const spy = vi.spyOn(console, `error`).mockImplementation(() => {})
    expect(await resolveAppUserForGithubActor({ id: 42 })).toBe(null)
    spy.mockRestore()
  })
})

describe(`recordGithubIdentity`, () => {
  beforeEach(() => {
    h.insertValues.length = 0
    h.conflictSets.length = 0
    h.insertRejects = false
  })

  it(`re-points user_id on conflict so a reconnect moves the mapping`, async () => {
    await recordGithubIdentity({
      userId: `u1`,
      githubUserId: 42,
      githubLogin: `niach`,
    })
    expect(h.insertValues[0]).toMatchObject({
      userId: `u1`,
      githubUserId: 42,
      githubLogin: `niach`,
    })
    expect(h.conflictSets[0]).toMatchObject({
      userId: `u1`,
      githubLogin: `niach`,
    })
  })

  it(`swallows a write failure — the connect flow must not break`, async () => {
    h.insertRejects = true
    const spy = vi.spyOn(console, `error`).mockImplementation(() => {})
    await expect(
      recordGithubIdentity({
        userId: `u1`,
        githubUserId: 42,
        githubLogin: `niach`,
      })
    ).resolves.toBeUndefined()
    spy.mockRestore()
  })
})
