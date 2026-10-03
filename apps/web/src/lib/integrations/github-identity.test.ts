import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the EXP-617 GitHub identity contract on its SLOP-7 storage: the
// Better Auth `github` account row. The single most important assertion is
// the rename guard: an actor resolves by NUMERIC id only — never by login,
// because GitHub logins are re-registerable and a login lookup would hand the
// notification-suppression decision to whoever squatted a freed name.

const h = vi.hoisted(() => ({
  byId: new Map<number, string>(),
  rejects: false,
  calls: [] as number[],
}))

vi.mock(`@/lib/integrations/github-user`, () => ({
  userIdForGithubAccountId: vi.fn(async (id: number) => {
    h.calls.push(id)
    if (h.rejects) throw new Error(`db down`)
    return h.byId.get(id) ?? null
  }),
}))

import {
  isBotActor,
  resolveAppUserForGithubActor,
} from "@/lib/integrations/github-identity"

describe(`isBotActor`, () => {
  it(`matches GitHub's Bot type and the [bot] login suffix`, () => {
    expect(isBotActor({ type: `Bot`, login: `exponential[bot]` })).toBe(true)
    expect(isBotActor({ login: `exponential[bot]` })).toBe(true)
    expect(isBotActor({ login: `Dependabot[BOT]` })).toBe(true)
    expect(isBotActor({ type: `User`, login: `niach` })).toBe(false)
    expect(isBotActor({})).toBe(false)
  })
})

describe(`resolveAppUserForGithubActor`, () => {
  beforeEach(() => {
    h.byId.clear()
    h.rejects = false
    h.calls.length = 0
  })

  it(`resolves a mapped numeric id`, async () => {
    h.byId.set(42, `u1`)
    expect(await resolveAppUserForGithubActor({ id: 42, login: `niach` })).toBe(
      `u1`
    )
    expect(h.calls).toEqual([42])
  })

  it(`returns null for an unmapped numeric id even when a login is present`, async () => {
    expect(await resolveAppUserForGithubActor({ id: 99, login: `niach` })).toBe(
      null
    )
  })

  it(`never queries for a bot`, async () => {
    h.byId.set(1, `u1`)
    expect(await resolveAppUserForGithubActor({ id: 1, login: `x[bot]` })).toBe(
      null
    )
    expect(h.calls).toEqual([])
  })

  it(`never falls back to a login match`, async () => {
    expect(await resolveAppUserForGithubActor({ login: `NiaCh` })).toBe(null)
    expect(h.calls).toEqual([])
  })

  it(`returns null for a missing actor or a query failure`, async () => {
    expect(await resolveAppUserForGithubActor(null)).toBe(null)
    expect(await resolveAppUserForGithubActor({})).toBe(null)

    h.rejects = true
    h.byId.set(42, `u1`)
    expect(await resolveAppUserForGithubActor({ id: 42 })).toBe(null)
  })
})
