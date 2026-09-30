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
