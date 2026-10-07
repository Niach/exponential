import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// The users router's key-kind gates. EXP-1140 follow-up: the launcher's
// hidden AGENT key (lib/auth/api-key-kind.ts) manages no API keys (minting a
// `personal` one would be its way around every kind-gated procedure), and
// NO `expu_` key, a person's included, changes who can sign in as the
// account (unlink a provider, delete a passkey, mint a link ticket). A
// cookie/bearer session (no api-key row behind `session.id`) passes every
// gate. The router runs against ctx.db, so the in-memory fake on the real
// schema tables is enough.

const h = vi.hoisted(() => ({
  createApiKey: vi.fn(async () => ({
    key: `expu_raw`,
    id: `new-key`,
    name: `Personal key`,
    start: `expu_`,
    prefix: `expu_`,
    createdAt: new Date(0),
  })),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: { api: { createApiKey: h.createApiKey } } }))
vi.mock(`@/lib/auth/resolve-bearer`, () => ({ invalidateSessionCache: vi.fn() }))
// FEED-76: the scope clamp reads membership through the cached module-db
// helper; the fake db below holds the teams/boards the names resolve from.
const T1 = `11111111-1111-4111-8111-111111111111`
const T2 = `22222222-2222-4222-8222-222222222222`
const B1 = `33333333-3333-4333-8333-333333333333`
const B_GONE = `44444444-4444-4444-8444-444444444444`
vi.mock(`@/lib/auth/membership`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/auth/membership")>()),
  getUserTeamIds: vi.fn(async () => [T1, T2]),
}))

import { usersRouter } from "@/lib/trpc/users"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"

let db: FakeDb

function callerFor(keyId?: string) {
  return usersRouter.createCaller({
    session: {
      user: { id: `actor`, name: `Actor`, email: `a@example.com` },
      session: { id: keyId ?? `sess-1` },
    },
    db,
    request: new Request(`http://localhost/`),
  } as never)
}

async function rejectionOf(promise: Promise<unknown>): Promise<TRPCError> {
  return promise.then(
    () => {
      throw new Error(`expected a rejection`)
    },
    (e: unknown) => e as TRPCError
  )
}

beforeEach(() => {
  h.createApiKey.mockClear()
  db = createFakeDb({
    apikeys: [
      { id: `key-agent`, referenceId: `actor`, metadata: JSON.stringify({ kind: `agent` }) },
      { id: `key-person`, referenceId: `actor`, metadata: JSON.stringify({ kind: `personal` }) },
    ],
    accounts: [{ id: `acc-1`, userId: `actor`, providerId: `google` }],
    passkeys: [{ id: `pk-1`, userId: `actor` }],
    teams: [
      { id: T1, name: `Acme` },
      { id: T2, name: `Lab` },
    ],
    // B_GONE is deliberately NOT seeded: the fake matches eq/inArray only
    // (never `boardVisible()`'s IS NULL), so a trashed board stands in as an
    // unknown id — the resolver drops both the same way.
    boards: [{ id: B1, teamId: T2, name: `Web`, prefix: `WEB` }],
  })
})

// FEED-76: a key minted with the consent screen's selection stores it
// (clamped) in its metadata and lists it by name; "Everything" stores no
// scope; the agent's hidden key is never scoped.
describe(`users — scoped API keys (FEED-76)`, () => {
  it(`mints a scoped key with the clamped selection and returns it by name`, async () => {
    const created = await callerFor().mintPersonalApiKey({
      name: `Bot`,
      scope: { teamIds: [T1, `55555555-5555-4555-8555-555555555555`], boardIds: [B1] },
    })
    expect(h.createApiKey).toHaveBeenCalledWith({
      body: expect.objectContaining({
        metadata: {
          kind: `personal`,
          scope: { allTeams: false, teamIds: [T1], boardIds: [B1] },
        },
      }),
    })
    expect(created.scope).toEqual({
      teams: [{ id: T1, name: `Acme` }],
      boards: [{ id: B1, name: `Web`, prefix: `WEB` }],
    })
  })

  it(`"Everything" and no scope both store an unscoped key`, async () => {
    const everything = await callerFor().mintPersonalApiKey({ scope: { allTeams: true } })
    expect(everything.scope).toBeNull()
    await callerFor().mintPersonalApiKey({ name: `Plain` })
    for (const call of h.createApiKey.mock.calls as unknown as Array<[{ body: { metadata: unknown } }]>) {
      expect(call[0].body.metadata).toEqual({ kind: `personal` })
    }
  })

  it(`refuses a scope on the agent purpose and a selection reaching nothing`, async () => {
    const agent = await rejectionOf(
      callerFor().mintPersonalApiKey({ purpose: `agent`, scope: { teamIds: [T1] } })
    )
    expect(agent.code).toBe(`BAD_REQUEST`)
    expect(agent.message).toBe(`An agent key cannot be scoped`)
    const empty = await rejectionOf(callerFor().mintPersonalApiKey({ scope: {} }))
    expect(empty.code).toBe(`BAD_REQUEST`)
    const foreign = await rejectionOf(
      callerFor().mintPersonalApiKey({
        scope: { teamIds: [`55555555-5555-4555-8555-555555555555`] },
      })
    )
    expect(foreign.code).toBe(`BAD_REQUEST`)
    expect(h.createApiKey).not.toHaveBeenCalled()
  })

  it(`lists the scope by name, dropping a vanished board, and never the raw metadata`, async () => {
    db.rows(`apikeys`).push({
      id: `key-scoped`,
      referenceId: `actor`,
      name: `Bot`,
      createdAt: new Date(1),
      metadata: JSON.stringify({
        kind: `personal`,
        scope: { allTeams: false, teamIds: [T1], boardIds: [B1, B_GONE] },
      }),
    })
    const { keys } = await callerFor().listPersonalApiKeys()
    const scoped = keys.find((row) => row.id === `key-scoped`)!
    expect(scoped.scope).toEqual({
      teams: [{ id: T1, name: `Acme` }],
      boards: [{ id: B1, name: `Web`, prefix: `WEB` }],
    })
    expect(`metadata` in scoped).toBe(false)
    expect(keys.find((row) => row.id === `key-person`)!.scope).toBeNull()
  })
})

describe(`users — API key management on the agent's key`, () => {
  it(`refuses mint, list and revoke on the agent key`, async () => {
    const agent = callerFor(`key-agent`)
    for (const call of [
      agent.mintPersonalApiKey({ purpose: `personal` }),
      agent.mintPersonalApiKey(),
      agent.listPersonalApiKeys(),
      agent.revokePersonalApiKey({ id: `key-person` }),
    ]) {
      const error = await rejectionOf(call)
      expect(error.code).toBe(`FORBIDDEN`)
      expect(error.message).toBe(`Agent keys cannot manage API keys`)
    }
    expect(h.createApiKey).not.toHaveBeenCalled()
    // The person's key row survived the refused revoke.
    expect(
      (await db.select().from((await import(`@/db/auth-schema`)).apikeys)).map((row) => row.id)
    ).toContain(`key-person`)
  })

  it(`a person's key and a real session still manage keys`, async () => {
    for (const keyId of [`key-person`, undefined]) {
      const result = await callerFor(keyId).mintPersonalApiKey({ name: `CLI` })
      expect(result.key).toBe(`expu_raw`)
      expect((await callerFor(keyId).listPersonalApiKeys()).keys.length).toBeGreaterThan(0)
    }
    expect(h.createApiKey).toHaveBeenCalledTimes(2)
  })
})

describe(`users — identity changes need a real session`, () => {
  it(`refuses unlink, passkey delete and the link ticket on ANY key`, async () => {
    for (const keyId of [`key-agent`, `key-person`]) {
      const caller = callerFor(keyId)
      for (const call of [
        caller.unlinkSignInMethod({ providerId: `google` }),
        caller.deletePasskey({ id: `pk-1` }),
        caller.mintSignInLinkTicket({ provider: `google` }),
      ]) {
        const error = await rejectionOf(call)
        expect(error.code).toBe(`UNAUTHORIZED`)
        expect(error.message).toContain(`not with an API key`)
      }
    }
    const schema = await import(`@/db/auth-schema`)
    expect(await db.select().from(schema.accounts)).toHaveLength(1)
    expect(await db.select().from(schema.passkeys)).toHaveLength(1)
  })

  it(`a cookie session passes the gate (and hits the endpoint's own rules)`, async () => {
    // No row named `nope`: the procedure ran past the gate to its own lookup.
    const error = await rejectionOf(callerFor().unlinkSignInMethod({ providerId: `nope` }))
    expect(error.code).toBe(`NOT_FOUND`)
    const passkeyError = await rejectionOf(callerFor().deletePasskey({ id: `nope` }))
    expect(passkeyError.code).toBe(`NOT_FOUND`)
  })
})
