import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the EXP-617 backfill's proof chain. The guards ARE the feature: this
// pass decides whose notifications get suppressed from data nobody re-verified
// at the time, so every rule that narrows it (personal accounts only, one
// unambiguous claimant, the numeric id from the installation object rather
// than a login, never overwriting an OAuth-recorded row) is load-bearing.

const h = vi.hoisted(() => ({
  candidateRows: [] as Array<Record<string, unknown>>,
  selectThrows: false,
  inserted: [] as Array<Record<string, unknown>>,
  // Rows the ON CONFLICT DO NOTHING is simulated to have swallowed.
  conflictOn: new Set<number>(),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    execute: vi.fn(async () => {
      if (h.selectThrows) throw new Error(`db down`)
      return { rows: h.candidateRows }
    }),
    insert: vi.fn(() => ({
      values: (v: Record<string, unknown>) => ({
        onConflictDoNothing: () => ({
          returning: async () => {
            if (h.conflictOn.has(v.githubUserId as number)) return []
            h.inserted.push(v)
            return [{ id: `row-1` }]
          },
        }),
      }),
    })),
  },
}))

const getInstallationAccount = vi.fn(
  async (
    _id: number
  ): Promise<{ id: number; login: string; type: string } | null> => ({
    id: 11409482,
    login: `Niach`,
    type: `User`,
  })
)
const githubAppConfigured = vi.fn(() => true)

vi.mock(`@/lib/integrations/github-app`, () => ({
  getInstallationAccount: (id: number) => getInstallationAccount(id),
  githubAppConfigured: () => githubAppConfigured(),
}))

vi.mock(`@/db/schema`, () => ({
  githubUserIdentities: { githubUserId: `github_user_id`, id: `id` },
}))

import { runGithubIdentityBackfill } from "@/lib/integrations/github-identity-backfill"

const candidate = (over: Record<string, unknown> = {}) => ({
  installation_id: 42,
  account_login: `Niach`,
  user_id: `u1`,
  ...over,
})

describe(`runGithubIdentityBackfill`, () => {
  beforeEach(() => {
    h.candidateRows.length = 0
    h.inserted.length = 0
    h.conflictOn.clear()
    h.selectThrows = false
    getInstallationAccount.mockClear()
    getInstallationAccount.mockResolvedValue({
      id: 11409482,
      login: `Niach`,
      type: `User`,
    })
    githubAppConfigured.mockReturnValue(true)
  })

  it(`maps a personal installation to its single claimant`, async () => {
    h.candidateRows.push(candidate())

    const result = await runGithubIdentityBackfill()

    expect(result).toEqual({ mapped: 1, skipped: 0 })
    expect(h.inserted).toEqual([
      { userId: `u1`, githubUserId: 11409482, githubLogin: `Niach` },
    ])
  })

  it(`takes the numeric id from the installation, not the stored login`, async () => {
    // The mirrored login is stale because the account was renamed. The id is
    // what makes that a non-event — a login lookup would have resolved
    // whoever holds `OldName` now.
    h.candidateRows.push(candidate({ account_login: `OldName` }))
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})

    await runGithubIdentityBackfill()

    expect(h.inserted[0]).toMatchObject({ githubUserId: 11409482 })
    expect(warn).toHaveBeenCalled()
    warn.mockRestore()
  })

  it(`skips an installation GitHub no longer reports as a personal account`, async () => {
    getInstallationAccount.mockResolvedValue({
      id: 99,
      login: `acme`,
      type: `Organization`,
    })
    h.candidateRows.push(candidate())

    expect(await runGithubIdentityBackfill()).toEqual({ mapped: 0, skipped: 1 })
    expect(h.inserted).toHaveLength(0)
  })

  it(`skips when the installation account cannot be read`, async () => {
    getInstallationAccount.mockResolvedValue(null)
    h.candidateRows.push(candidate())

    expect(await runGithubIdentityBackfill()).toEqual({ mapped: 0, skipped: 1 })
  })

  it(`never overwrites an identity the OAuth callback already recorded`, async () => {
    h.conflictOn.add(11409482)
    h.candidateRows.push(candidate())

    expect(await runGithubIdentityBackfill()).toEqual({ mapped: 0, skipped: 1 })
    expect(h.inserted).toHaveLength(0)
  })

  it(`does nothing without a configured GitHub App`, async () => {
    githubAppConfigured.mockReturnValue(false)
    h.candidateRows.push(candidate())

    expect(await runGithubIdentityBackfill()).toEqual({ mapped: 0, skipped: 0 })
    expect(getInstallationAccount).not.toHaveBeenCalled()
  })

  it(`swallows a query failure — a boot must never fail on this`, async () => {
    h.selectThrows = true
    const spy = vi.spyOn(console, `error`).mockImplementation(() => {})

    await expect(runGithubIdentityBackfill()).resolves.toEqual({
      mapped: 0,
      skipped: 0,
    })
    spy.mockRestore()
  })
})
