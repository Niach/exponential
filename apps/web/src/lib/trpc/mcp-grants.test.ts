import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// The mcp-grants router reads the MODULE-LEVEL `db` from @/db/connection (not
// ctx.db), so the fake db is installed through the mock rather than the caller
// ctx. `select()` shifts pre-seeded rows off a FIFO queue; `insert().values()
// .onConflictDoUpdate()` records the upserted values and stamps `upsert` on a
// shared call log; `oAuthConsent` stamps `consent`. The call log is the
// invariant under test — consent must complete before the grant upsert.
const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const upserts: Record<string, unknown>[] = []
  const callLog: string[] = []
  const whereArgs: unknown[] = []
  const oAuthConsent = vi.fn()

  function selectChain(): Promise<unknown[]> &
    Record<string, (...args: unknown[]) => unknown> {
    const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
      Record<string, (...args: unknown[]) => unknown>
    for (const m of [`from`, `innerJoin`, `orderBy`, `limit`]) {
      p[m] = () => p
    }
    // The fake never evaluates SQL — record the where clause so user-scoping
    // can still be asserted (see findParamValues below).
    p.where = (...args: unknown[]) => {
      whereArgs.push(args[0])
      return p
    }
    return p
  }

  const fakeDb = {
    select: () => selectChain(),
    insert: () => ({
      values: (v: Record<string, unknown>) => ({
        onConflictDoUpdate: () => {
          upserts.push(v)
          callLog.push(`upsert`)
          return Promise.resolve()
        },
      }),
    }),
  }

  return { selectQueue, upserts, callLog, whereArgs, oAuthConsent, fakeDb }
})

vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
vi.mock(`@/lib/auth`, () => ({
  auth: { api: { oAuthConsent: h.oAuthConsent } },
}))

import { mcpGrantsRouter } from "@/lib/trpc/mcp-grants"

const REDIRECT = `https://client.example/cb?code=x`
const WS1 = `11111111-1111-4111-8111-111111111111`
const WS2 = `22222222-2222-4222-8222-222222222222`
const PROJ1 = `33333333-3333-4333-8333-333333333333`

function caller() {
  return mcpGrantsRouter.createCaller({
    session: { user: { id: `actor` } },
    db: h.fakeDb,
    request: new Request(`http://localhost/`),
  } as never)
}

// Recursively collect every bound parameter value from a drizzle SQL object
// (Param nodes carry a string `value`) — how the tests see what a recorded
// where clause was actually scoped to.
function findParamValues(
  node: unknown,
  out: string[] = [],
  seen = new Set<object>()
): string[] {
  if (!node || typeof node !== `object` || seen.has(node)) return out
  seen.add(node)
  const record = node as Record<string, unknown>
  if (typeof record.value === `string`) out.push(record.value)
  for (const value of Object.values(record)) findParamValues(value, out, seen)
  return out
}

beforeEach(() => {
  h.selectQueue.length = 0
  h.upserts.length = 0
  h.callLog.length = 0
  h.whereArgs.length = 0
  h.oAuthConsent.mockReset()
  // Default: consent succeeds and returns the client callback URL.
  h.oAuthConsent.mockImplementation(async () => {
    h.callLog.push(`consent`)
    return { redirectURI: REDIRECT }
  })
})

describe(`mcpGrants.hasAny — user-level MCP-connected existence check (EXP-141)`, () => {
  it(`returns false when the user has no grant rows`, async () => {
    h.selectQueue.push([])
    await expect(caller().hasAny()).resolves.toEqual({ hasAny: false })
  })

  it(`returns true when the user has a grant row`, async () => {
    h.selectQueue.push([{ id: `grant-1` }])
    await expect(caller().hasAny()).resolves.toEqual({ hasAny: true })
  })

  it(`scopes the lookup to the calling user — someone else's rows never count`, async () => {
    // The fake db can't evaluate SQL, so "another user's row only" is asserted
    // structurally: the recorded where clause must bind the CALLER's id — the
    // filter that keeps foreign rows out of the real query.
    h.selectQueue.push([{ id: `grant-of-someone-else` }])
    await caller().hasAny()
    expect(h.whereArgs).toHaveLength(1)
    expect(findParamValues(h.whereArgs[0])).toContain(`actor`)
  })
})

describe(`mcpGrants.grantAndConsent — consent completes before the grant upsert`, () => {
  it(`accept with an expired/replayed consent code rejects and never rewrites the grant`, async () => {
    h.selectQueue.push([{ clientId: `client-1` }]) // client lookup
    h.selectQueue.push([]) // member workspaces
    h.oAuthConsent.mockImplementation(async () => {
      h.callLog.push(`consent`)
      throw new TRPCError({ code: `UNAUTHORIZED`, message: `expired` })
    })

    await expect(
      caller().grantAndConsent({
        clientId: `client-1`,
        consentCode: `code-1`,
        accept: true,
        allWorkspaces: true,
      })
    ).rejects.toThrow()

    // The bug this fixes: a failed consent used to still rewrite the grant.
    expect(h.upserts).toHaveLength(0)
    expect(h.callLog).toEqual([`consent`])
  })

  it(`accept where consent returns no redirectURI rejects BAD_REQUEST with no upsert`, async () => {
    h.selectQueue.push([{ clientId: `client-1` }])
    h.selectQueue.push([])
    h.oAuthConsent.mockImplementation(async () => {
      h.callLog.push(`consent`)
      return {}
    })

    await expect(
      caller().grantAndConsent({
        clientId: `client-1`,
        consentCode: `code-1`,
        accept: true,
        allWorkspaces: true,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    expect(h.upserts).toHaveLength(0)
  })

  it(`accept persists the clamped selection only after consent completes`, async () => {
    h.selectQueue.push([{ clientId: `client-1` }]) // client lookup
    h.selectQueue.push([{ id: WS1 }, { id: WS2 }]) // member workspaces
    h.selectQueue.push([{ id: PROJ1, workspaceId: WS2 }]) // project clamp

    const result = await caller().grantAndConsent({
      clientId: `client-1`,
      consentCode: `code-1`,
      accept: true,
      allWorkspaces: false,
      workspaceIds: [WS1],
      projectIds: [PROJ1],
    })

    expect(result).toEqual({ redirectURI: REDIRECT })
    // Order is the invariant: consent (which mints the code) then the upsert.
    expect(h.callLog).toEqual([`consent`, `upsert`])
    expect(h.upserts).toHaveLength(1)
    expect(h.upserts[0]).toMatchObject({
      userId: `actor`,
      clientId: `client-1`,
      allWorkspaces: false,
      workspaceIds: [WS1],
      projectIds: [PROJ1],
    })
  })

  it(`deny completes consent negatively, writes no grant, and reads no db`, async () => {
    h.selectQueue.push([{ sentinel: true }])

    const result = await caller().grantAndConsent({
      clientId: `client-1`,
      consentCode: `code-1`,
      accept: false,
    })

    expect(result).toEqual({ redirectURI: REDIRECT })
    expect(h.upserts).toHaveLength(0)
    expect(h.callLog).toEqual([`consent`])
    // The deny path short-circuits before any select — queue is untouched.
    expect(h.selectQueue).toHaveLength(1)
  })

  it(`accept with an empty selection rejects before consuming the consent code`, async () => {
    await expect(
      caller().grantAndConsent({
        clientId: `client-1`,
        consentCode: `code-1`,
        accept: true,
        allWorkspaces: false,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    // Validation failures must leave the consent code alive for a retry.
    expect(h.oAuthConsent).not.toHaveBeenCalled()
    expect(h.upserts).toHaveLength(0)
    expect(h.callLog).toEqual([])
  })
})
