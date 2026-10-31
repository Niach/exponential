import { afterEach, beforeEach, describe, expect, it } from "vitest"
import fixture from "@exp/emcp/fixtures/vault.json" with { type: "json" }

// #EXP-1491: better-auth reads and writes plaintext `accounts` tokens; the
// adapter wrapper seals them at rest, bound to the row id.
import { withAccountTokenVault } from "@/lib/emcp/account-vault"
import { openAccountToken } from "@/lib/emcp/vault"

type Row = Record<string, unknown>

function fakeAdapter() {
  const rows: Row[] = []
  const match = (row: Row, where: Array<{ field: string; value: unknown }>) =>
    where.every((w) => row[w.field] === w.value)
  const adapter = {
    id: `fake`,
    rows,
    create: async ({ data, forceAllowId }: { model: string; data: Row; forceAllowId?: boolean }) => {
      const row = { ...data, id: forceAllowId && data.id ? data.id : `generated` }
      rows.push(row)
      return { ...row }
    },
    findOne: async ({ where }: { model: string; where: Array<{ field: string; value: unknown }> }) => {
      const row = rows.find((r) => match(r, where))
      return row ? { ...row } : null
    },
    findMany: async ({ where = [] }: { model: string; where?: Array<{ field: string; value: unknown }> }) =>
      rows.filter((r) => match(r, where)).map((r) => ({ ...r })),
    update: async ({ where, update }: { model: string; where: Array<{ field: string; value: unknown }>; update: Row }) => {
      const row = rows.find((r) => match(r, where))
      if (!row) return null
      Object.assign(row, update)
      return { ...row }
    },
  }
  return adapter
}

const saved: Record<string, string | undefined> = {}
beforeEach(() => {
  saved.VAULT_KEY = process.env.VAULT_KEY
  process.env.VAULT_KEY = fixture.keys.k1
})
afterEach(() => {
  if (saved.VAULT_KEY === undefined) delete process.env.VAULT_KEY
  else process.env.VAULT_KEY = saved.VAULT_KEY
})

describe(`withAccountTokenVault`, () => {
  it(`seals on create under a minted id, returns plaintext`, async () => {
    const inner = fakeAdapter()
    const adapter = withAccountTokenVault(() => inner)()
    const created = (await adapter.create({
      model: `account`,
      data: { userId: `u1`, providerId: `github`, accessToken: `gho_a`, refreshToken: `ghr_r`, idToken: null },
      forceAllowId: true,
    })) as Row
    expect(created.accessToken).toBe(`gho_a`)
    const stored = inner.rows[0]!
    expect(stored.id).toMatch(/^[A-Za-z0-9]{32}$/)
    expect(String(stored.accessToken).startsWith(`ev1.k1.`)).toBe(true)
    expect(openAccountToken(stored.refreshToken as string, stored.id as string, `refresh_token`)).toBe(`ghr_r`)
    expect(stored.idToken).toBeNull()
    expect(((await adapter.findOne({ model: `account`, where: [{ field: `id`, value: stored.id }] })) as Row).accessToken).toBe(`gho_a`)
  })

  it(`seals on update (by id, or by another key through a lookup)`, async () => {
    const inner = fakeAdapter()
    inner.rows.push({ id: `acc_1`, userId: `u1`, providerId: `github`, accessToken: `gho_old` })
    const adapter = withAccountTokenVault(() => inner)()
    const updated = (await adapter.update({
      model: `account`,
      where: [{ field: `userId`, value: `u1` }],
      update: { accessToken: `gho_new` },
    })) as Row
    expect(updated.accessToken).toBe(`gho_new`)
    expect(openAccountToken(inner.rows[0]!.accessToken as string, `acc_1`, `access_token`)).toBe(`gho_new`)
  })

  it(`reads a pre-EXP-1491 plaintext token as itself, and leaves other models alone`, async () => {
    const inner = fakeAdapter()
    inner.rows.push({ id: `acc_1`, userId: `u1`, accessToken: `gho_plain` })
    const adapter = withAccountTokenVault(() => inner)()
    expect(((await adapter.findMany({ model: `account`, where: [] })) as Row[])[0]!.accessToken).toBe(`gho_plain`)
    const session = (await adapter.create({ model: `session`, data: { token: `t` } })) as Row
    expect(session.token).toBe(`t`)
  })
})
