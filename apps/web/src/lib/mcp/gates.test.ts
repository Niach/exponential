import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// EXP-660: resolveMcpToolGates decides per request whether the helpdesk tool
// family registers at all. It must stay cheap (one indexed select at most)
// and must intersect the caller's membership with the OAuth grant, because
// the helpdesk tools themselves require a FULL team grant.
//
// EXP-679: it also decides whether exponential_sessions_end registers — only
// for the caller's OWN run, and only when an automation or another agent
// started it (started_reason set).

const h = vi.hoisted(() => {
  const dbRows: { current: Array<unknown> } = { current: [] }
  const state: { capturedWhere: unknown } = { capturedWhere: undefined }
  const queryBuilder: Record<string, unknown> = {}
  for (const method of [`from`, `limit`]) {
    queryBuilder[method] = vi.fn(() => queryBuilder)
  }
  queryBuilder.where = vi.fn((cond: unknown) => {
    state.capturedWhere = cond
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

import { resolveMcpToolGates } from "@/lib/mcp/gates"
import { FULL_ACCESS, type McpAccess } from "@/lib/mcp/scope"

const WS = `22222222-2222-2222-2222-222222222222`
const OTHER = `33333333-3333-3333-3333-333333333333`

function renderWhere(): { sql: string; params: unknown[] } {
  const query = new PgDialect().sqlToQuery(h.state.capturedWhere as never)
  return { sql: query.sql, params: query.params }
}

beforeEach(() => {
  vi.clearAllMocks()
  h.dbRows.current = []
  h.state.capturedWhere = undefined
  h.getUserTeamIds.mockResolvedValue([])
})

describe(`resolveMcpToolGates`, () => {
  it(`is off without a single member team, and never queries`, async () => {
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS)).toEqual({
      helpdesk: false,
      sessionsEnd: false,
    })
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`is on when a member team has helpdesk enabled`, async () => {
    h.getUserTeamIds.mockResolvedValue([WS])
    h.dbRows.current = [{ id: WS }]
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS)).toEqual({
      helpdesk: true,
      sessionsEnd: false,
    })
    const { sql, params } = renderWhere()
    expect(sql).toContain(`"id" in`)
    expect(sql).toContain(`"helpdesk_enabled" =`)
    expect(params).toContain(WS)
  })

  it(`is off when no member team has it enabled`, async () => {
    h.getUserTeamIds.mockResolvedValue([WS, OTHER])
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS)).toEqual({
      helpdesk: false,
      sessionsEnd: false,
    })
  })

  it(`intersects membership with the token's FULL team grants`, async () => {
    h.getUserTeamIds.mockResolvedValue([WS, OTHER])
    h.dbRows.current = [{ id: WS }]
    const scoped: McpAccess = {
      full: false,
      fullTeamIds: new Set([WS]),
      grantedBoardIds: new Set(),
      visibleTeamIds: new Set([WS, OTHER]),
    }
    expect(await resolveMcpToolGates(`u`, scoped)).toEqual({
      helpdesk: true,
      sessionsEnd: false,
    })
    const { params } = renderWhere()
    expect(params).toContain(WS)
    // OTHER is only board-visible — the helpdesk tools would refuse it, so
    // it must not be what switches the family on.
    expect(params).not.toContain(OTHER)
  })

  it(`skips the query when the grant leaves no full team`, async () => {
    h.getUserTeamIds.mockResolvedValue([OTHER])
    const scoped: McpAccess = {
      full: false,
      fullTeamIds: new Set([WS]),
      grantedBoardIds: new Set(),
      visibleTeamIds: new Set([WS, OTHER]),
    }
    expect(await resolveMcpToolGates(`u`, scoped)).toEqual({
      helpdesk: false,
      sessionsEnd: false,
    })
    expect(h.db.select).not.toHaveBeenCalled()
  })
})

describe(`resolveMcpToolGates — sessionsEnd (EXP-679)`, () => {
  const RUN = `44444444-4444-4444-4444-444444444444`

  it(`is off without a session header, and never queries`, async () => {
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, null)
    expect(gates.sessionsEnd).toBe(false)
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`is off for a person-started run`, async () => {
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, startedReason: null },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates.sessionsEnd).toBe(false)
  })

  it(`is on for the caller's own automation-started run`, async () => {
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, startedReason: `schedule` },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates.sessionsEnd).toBe(true)
    const { sql, params } = renderWhere()
    expect(sql).toContain(`"id" =`)
    expect(params).toContain(RUN)
  })

  it(`is off when the run is someone else's`, async () => {
    h.dbRows.current = [
      { userId: `other`, hostUserId: `other-host`, startedReason: `schedule` },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates.sessionsEnd).toBe(false)
  })

  it(`is off when the header names no row at all`, async () => {
    h.dbRows.current = []
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates.sessionsEnd).toBe(false)
  })
})
