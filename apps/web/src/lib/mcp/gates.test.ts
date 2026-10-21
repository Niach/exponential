import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// EXP-660: resolveMcpToolGates decides per request which gated tools
// register at all. It must stay cheap (one indexed select at most). SLOP-4
// retired the helpdesk gate; what remains are the session-header gates.
//
// EXP-1291: one header gate (`inSession`: the caller's OWN session, starter
// or host) plus two wording flags: unattended (a child, or a root another
// session spawned) and agent (the root is a team agent).

const h = vi.hoisted(() => {
  const dbRows: { current: Array<unknown> } = { current: [] }
  const state: { capturedWhere: unknown } = { capturedWhere: undefined }
  const queryBuilder: Record<string, unknown> = {}
  for (const method of [`from`, `leftJoin`, `limit`]) {
    queryBuilder[method] = vi.fn(() => queryBuilder)
  }
  queryBuilder.where = vi.fn((cond: unknown) => {
    // The FIRST where of a resolve: the row lookup keyed on the header's id.
    state.capturedWhere ??= cond
    return queryBuilder
  })
  ;(queryBuilder as { then: unknown }).then = (
    resolve: (v: unknown) => unknown,
    reject: (e: unknown) => unknown
  ) => Promise.resolve(dbRows.current).then(resolve, reject)
  const db = { select: vi.fn(() => queryBuilder) }
  const getUserTeamIds = vi.fn(async (): Promise<string[]> => [])
  return { dbRows, state, db, getUserTeamIds }
})

vi.mock(`@/db/connection`, () => ({ db: h.db }))
vi.mock(`@/lib/team-membership`, () => ({ getUserTeamIds: h.getUserTeamIds }))
// NEW-1: the host is derived from the devices rows; the mocked rows carry
// it as `hostUserId` so the cases read as before.
vi.mock(`@/lib/sessions/host`, () => ({
  isSessionPrincipal: async (userId: string, row: { userId: string; hostUserId?: string | null }) =>
    row.userId === userId || row.hostUserId === userId,
  sessionDeviceOwner: async (row: { userId: string; hostUserId?: string | null }) =>
    row.hostUserId ?? row.userId,
}))

import { resolveMcpToolGates } from "@/lib/mcp/gates"
import { FULL_ACCESS, type McpAccess } from "@/lib/mcp/scope"

const WS = `22222222-2222-2222-2222-222222222222`

function renderWhere(): { sql: string; params: unknown[] } {
  const query = new PgDialect().sqlToQuery(h.state.capturedWhere as never)
  return { sql: query.sql, params: query.params }
}

beforeEach(() => {
  vi.clearAllMocks()
  h.dbRows.current = []
  h.state.capturedWhere = undefined
})

const CLOSED = { inSession: false, unattended: false, agent: false }
const RUN = `44444444-4444-4444-4444-444444444444`
const PARENT = `55555555-5555-4555-8555-555555555555`

describe(`resolveMcpToolGates`, () => {
  it(`is all off without a session header, and never queries`, async () => {
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS)).toEqual(CLOSED)
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`ignores the OAuth grant`, async () => {
    const scoped: McpAccess = {
      full: false,
      fullTeamIds: new Set([WS]),
      grantedBoardIds: new Set(),
      visibleTeamIds: new Set([WS]),
    }
    expect(await resolveMcpToolGates(`u`, scoped)).toEqual(CLOSED)
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`is on and attended for the caller's own person-started root`, async () => {
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, parentId: null, rootKind: `thread`, watched: false, planMode: false },
    ]
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).toEqual({
      inSession: true,
      unattended: false,
      agent: false,
      planMode: false,
      cards: true,
    })
    // Two indexed selects: the row (keyed on the header's id) and its host
    // device's version.
    expect(h.db.select).toHaveBeenCalledTimes(2)
    const { sql, params } = renderWhere()
    expect(sql).toContain(`"id" =`)
    expect(params).toContain(RUN)
  })

  it(`is unattended for a child and for a root a live session watches (its spawner)`, async () => {
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, parentId: PARENT, rootKind: `thread`, watched: false, planMode: false },
    ]
    expect((await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).unattended).toBe(true)
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, parentId: null, rootKind: `thread`, watched: true, planMode: false },
    ]
    expect((await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).unattended).toBe(true)
  })

  it(`flags a team agent's session`, async () => {
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, parentId: null, rootKind: `agent`, watched: false, planMode: false },
    ]
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).toEqual({
      inSession: true,
      unattended: false,
      agent: true,
      planMode: false,
      cards: true,
    })
  })

  // EXP-1280: a host below 0.14.73 cannot feed the session's own cards back.
  it(`drops the card tools for a host that cannot deliver their answer`, async () => {
    const row = {
      userId: `u`,
      hostUserId: null,
      parentId: null,
      rootKind: `thread`,
      watched: false,
      planMode: false,
    }
    h.dbRows.current = [{ ...row, version: `0.14.72` }]
    expect((await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).cards).toBe(false)
    h.dbRows.current = [{ ...row, version: `0.14.73` }]
    expect((await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).cards).toBe(true)
    // Unreported fails open (the client-version rule).
    h.dbRows.current = [{ ...row, version: null }]
    expect((await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).cards).toBe(true)
  })

  it(`flags a session that is still planning`, async () => {
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, parentId: null, rootKind: `thread`, watched: false, planMode: true },
    ]
    expect((await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).planMode).toBe(true)
  })

  it(`is on for a session the caller only hosts`, async () => {
    h.dbRows.current = [
      { userId: `owner`, hostUserId: `u`, parentId: null, rootKind: `thread`, watched: false, planMode: false },
    ]
    expect((await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).inSession).toBe(true)
  })

  it(`is off for someone else's session and for a header naming no row`, async () => {
    h.dbRows.current = [
      { userId: `other`, hostUserId: `other-host`, parentId: null, rootKind: `thread`, watched: false, planMode: false },
    ]
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).toEqual(CLOSED)
    h.dbRows.current = []
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).toEqual(CLOSED)
  })
})
