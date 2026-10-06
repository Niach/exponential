import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// EXP-660: resolveMcpToolGates decides per request which gated tools
// register at all. It must stay cheap (one indexed select at most). SLOP-4
// retired the helpdesk gate; what remains are the session-header gates.
//
// EXP-679 / EXP-1222: it decides whether exponential_sessions_end registers —
// for any of the caller's OWN runs — and whether that run is unattended
// (started_reason set), which only picks the instructions' wording.
//
// Compat shim (release train 2026-10-06): an ATTENDED run on a daemon older
// than 0.14.63 does NOT get the tool (that playbook would end the run at
// close-out), so the rows below carry the host device's `deviceVersion`.
// Delete the shim cases once CLIENT_MIN_VERSION_DESKTOP and _CLI are
// >= 0.14.63.

const h = vi.hoisted(() => {
  const dbRows: { current: Array<unknown> } = { current: [] }
  const state: { capturedWhere: unknown } = { capturedWhere: undefined }
  const queryBuilder: Record<string, unknown> = {}
  for (const method of [`from`, `leftJoin`, `limit`]) {
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

function renderWhere(): { sql: string; params: unknown[] } {
  const query = new PgDialect().sqlToQuery(h.state.capturedWhere as never)
  return { sql: query.sql, params: query.params }
}

beforeEach(() => {
  vi.clearAllMocks()
  h.dbRows.current = []
  h.state.capturedWhere = undefined
})

describe(`resolveMcpToolGates`, () => {
  it(`is all off without a session header, and never queries`, async () => {
    expect(await resolveMcpToolGates(`u`, FULL_ACCESS)).toEqual({
      sessionsEnd: false,
      unattended: false,
      askParent: false,
      sessionResults: false,
    })
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`ignores the OAuth grant — no gate is team-scoped since SLOP-4`, async () => {
    const scoped: McpAccess = {
      full: false,
      fullTeamIds: new Set([WS]),
      grantedBoardIds: new Set(),
      visibleTeamIds: new Set([WS]),
    }
    expect(await resolveMcpToolGates(`u`, scoped)).toEqual({
      sessionsEnd: false,
      unattended: false,
      askParent: false,
      sessionResults: false,
    })
    expect(h.db.select).not.toHaveBeenCalled()
  })
})

describe(`resolveMcpToolGates — sessionsEnd (EXP-679, EXP-1222)`, () => {
  const RUN = `44444444-4444-4444-4444-444444444444`

  it(`is off without a session header, and never queries`, async () => {
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, null)
    expect(gates).toMatchObject({ sessionsEnd: false, unattended: false })
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`is on for a person-started run on a current daemon, which is not unattended`, async () => {
    h.dbRows.current = [
      {
        userId: `u`,
        hostUserId: null,
        startedReason: null,
        deviceVersion: `0.14.63`,
      },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates).toMatchObject({ sessionsEnd: true, unattended: false })
    // The host device's row rides the one lookup (no second query).
    expect(h.db.select).toHaveBeenCalledTimes(1)
  })

  // Compat shim: delete once CLIENT_MIN_VERSION_DESKTOP and _CLI are >= 0.14.63.
  it(`is OFF for a person-started run on a daemon older than 0.14.63 (its playbook would end it at close-out)`, async () => {
    for (const deviceVersion of [`0.14.62`, `0.14.61`, `0.13.99`, `0.14.62-staging`]) {
      h.dbRows.current = [
        { userId: `u`, hostUserId: null, startedReason: null, deviceVersion },
      ]
      const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
      expect(gates, deviceVersion).toMatchObject({
        sessionsEnd: false,
        unattended: false,
        askParent: true,
        sessionResults: true,
      })
    }
  })

  it(`treats a missing or unparseable device version as OLD for an attended run`, async () => {
    for (const deviceVersion of [null, undefined, ``, `dev`, `v0.14.63`]) {
      h.dbRows.current = [
        { userId: `u`, hostUserId: null, startedReason: null, deviceVersion },
      ]
      const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
      expect(gates.sessionsEnd, String(deviceVersion)).toBe(false)
    }
  })

  it(`is on for a person-started run on any daemon past 0.14.63`, async () => {
    for (const deviceVersion of [`0.14.64`, `0.15.0`, `1.0.0`, `0.14.63-staging`]) {
      h.dbRows.current = [
        { userId: `u`, hostUserId: null, startedReason: null, deviceVersion },
      ]
      const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
      expect(gates.sessionsEnd, deviceVersion).toBe(true)
    }
  })

  it(`is on for an unattended run whatever the daemon's version`, async () => {
    for (const deviceVersion of [`0.14.61`, null, `dev`]) {
      h.dbRows.current = [
        { userId: `u`, hostUserId: null, startedReason: `agent`, deviceVersion },
      ]
      const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
      expect(gates, String(deviceVersion)).toMatchObject({
        sessionsEnd: true,
        unattended: true,
      })
    }
  })

  it(`is on for the caller's own automation-started run, unattended`, async () => {
    h.dbRows.current = [
      { userId: `u`, hostUserId: null, startedReason: `schedule` },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates).toMatchObject({ sessionsEnd: true, unattended: true })
    const { sql, params } = renderWhere()
    expect(sql).toContain(`"id" =`)
    expect(params).toContain(RUN)
  })

  it(`is off when the run is someone else's`, async () => {
    h.dbRows.current = [
      { userId: `other`, hostUserId: `other-host`, startedReason: `schedule` },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates).toMatchObject({ sessionsEnd: false, unattended: false })
  })

  it(`is off when the header names no row at all`, async () => {
    h.dbRows.current = []
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates.sessionsEnd).toBe(false)
  })
})

// EXP-700 / EXP-1089: askParent opens for EVERY run of the caller's (the
// handler decides who may be asked: `to: 'user'` from any run, a starter only
// where there is one). The parent linkage is deliberately NOT part of the
// gate: the parent stamps parent_session_id only after its sessions_start
// poll returns, so a child whose tools/list wins that race would otherwise
// never see the tool.
describe(`resolveMcpToolGates — askParent (EXP-700, EXP-1089)`, () => {
  const RUN = `44444444-4444-4444-4444-444444444444`
  const PARENT = `55555555-5555-4555-8555-555555555555`

  it(`is off without a session header`, async () => {
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, null)
    expect(gates.askParent).toBe(false)
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`is on for the caller's own agent-started run with a linked parent`, async () => {
    h.dbRows.current = [
      {
        userId: `u`,
        hostUserId: null,
        startedReason: `agent`,
        parentSessionId: PARENT,
      },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates).toMatchObject({ sessionsEnd: true, askParent: true })
  })

  it(`is on for an automation-started run (it may ask the person)`, async () => {
    h.dbRows.current = [
      {
        userId: `u`,
        hostUserId: null,
        startedReason: `schedule`,
        parentSessionId: null,
      },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates).toMatchObject({ sessionsEnd: true, askParent: true })
  })

  it(`is on for the caller's own person-started run, attended`, async () => {
    // EXP-1089: a workflow's planner run pressed from the page is one of
    // these — it clears its questions with the person before the graph exists.
    h.dbRows.current = [
      {
        userId: `u`,
        hostUserId: null,
        startedReason: null,
        parentSessionId: null,
        deviceVersion: `0.14.63`,
      },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates).toMatchObject({
      sessionsEnd: true,
      unattended: false,
      askParent: true,
    })
  })

  it(`is on for an agent-started run whose parent is not stamped YET`, async () => {
    // The race the gate must not lose: the row exists (the device created it)
    // before the parent's sessions_start poll stamps parent_session_id.
    h.dbRows.current = [
      {
        userId: `u`,
        hostUserId: null,
        startedReason: `agent`,
        parentSessionId: null,
      },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates).toMatchObject({ sessionsEnd: true, askParent: true })
  })

  it(`is off when the run is someone else's`, async () => {
    h.dbRows.current = [
      {
        userId: `other`,
        hostUserId: `other-host`,
        startedReason: `agent`,
        parentSessionId: PARENT,
      },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates.askParent).toBe(false)
  })
})

// EXP-879: sessionResults opens for ANY run of the caller's — attended or
// not. The only questions are "is there a run" and "is it mine".
describe(`resolveMcpToolGates — sessionResults (EXP-879)`, () => {
  const RUN = `44444444-4444-4444-4444-444444444444`

  it(`is off without a session header, and never queries`, async () => {
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, null)
    expect(gates.sessionResults).toBe(false)
    expect(h.db.select).not.toHaveBeenCalled()
  })

  it(`is on for the caller's own PERSON-started run`, async () => {
    h.dbRows.current = [
      {
        userId: `u`,
        hostUserId: null,
        startedReason: null,
        deviceVersion: `0.14.63`,
      },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    // EXP-1222: an attended run gets the close-out and publishing alike.
    expect(gates).toMatchObject({
      sessionsEnd: true,
      unattended: false,
      sessionResults: true,
    })
  })

  it(`is on for a run the caller only hosts`, async () => {
    h.dbRows.current = [
      { userId: `owner`, hostUserId: `u`, startedReason: `schedule` },
    ]
    const gates = await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)
    expect(gates.sessionResults).toBe(true)
  })

  it(`is off for someone else's run and for a header naming no row`, async () => {
    h.dbRows.current = [
      { userId: `other`, hostUserId: `other-host`, startedReason: null },
    ]
    expect(
      (await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).sessionResults
    ).toBe(false)
    h.dbRows.current = []
    expect(
      (await resolveMcpToolGates(`u`, FULL_ACCESS, RUN)).sessionResults
    ).toBe(false)
  })
})
