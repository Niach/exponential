import { describe, expect, it } from "vitest"
import fixture from "@exp/emcp/fixtures/audit.json" with { type: "json" }
import { auditExpired } from "@exp/emcp/audit"

// #EXP-1491: the gate's `mcp_calls` rows replay @exp/emcp's audit.json (the
// args hash, the status order, retention) and classify the built-in tools'
// answers without reading their messages into the row.
import { AUDIT_RETENTION_MS, auditRows, BUILTIN_CALL_ERRORS, classifyToolResponse, toolCallsOf } from "@/lib/emcp/audit"
import { auditTeamId } from "@/lib/emcp/gate"
import { FULL_ACCESS, NO_ACCESS } from "@/lib/emcp/scope"

const caller = { userId: `u1`, clientId: `c1`, teamId: null, appId: null }
const call = (id: number, name: string, args?: unknown) => ({
  jsonrpc: `2.0`,
  id,
  method: `tools/call`,
  params: { name, ...(args === undefined ? {} : { arguments: args }) },
})
const ok = (id: number) => ({ jsonrpc: `2.0`, id, result: { content: [{ type: `text`, text: `{}` }] } })
const toolError = (id: number, text: string) => ({
  jsonrpc: `2.0`,
  id,
  result: { content: [{ type: `text`, text }], isError: true },
})

describe(`audit.json replay`, () => {
  it(`hashes the arguments as the fixture does`, () => {
    for (const entry of fixture.hashCases) {
      const [row] = auditRows(caller, call(1, `exponential_issues`, entry.args ?? undefined), ok(1), 3)
      expect(row!.argsHash).toBe(entry.expected)
    }
  })

  it(`keeps the 90-day retention`, () => {
    expect(AUDIT_RETENTION_MS).toBe(fixture.retentionDays * 86_400_000)
    for (const entry of fixture.retentionCases) {
      const expired = Date.parse(entry.now) - Date.parse(entry.createdAt) >= AUDIT_RETENTION_MS
      expect(expired).toBe(entry.expected)
      expect(auditExpired(entry.createdAt, entry.now)).toBe(entry.expected)
    }
  })

  it(`writes exactly the fixture's columns`, () => {
    const [row] = auditRows(caller, call(1, `exponential_issues`, {}), ok(1), 12.9)
    const columns = Object.keys({ id: 0, createdAt: 0, ...row })
      .map((key) => key.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`))
      .sort()
    expect(columns).toEqual([...fixture.columns].sort())
    expect(row!.latencyMs).toBe(12)
    expect(fixture.statuses).toContain(row!.status)
  })
})

describe(`classifying a built-in call`, () => {
  it(`ok, failed, denied (unknown tool, not granted), invalid, no answer`, () => {
    expect(classifyToolResponse(ok(1))).toEqual({ outcome: {}, error: null })
    expect(classifyToolResponse(toolError(1, `Issue EXP-9 not found`))).toEqual({
      outcome: { error: true },
      error: BUILTIN_CALL_ERRORS.toolError,
    })
    expect(classifyToolResponse(toolError(1, `MCP error -32602: Tool exponential_nope not found`)).error).toBe(
      BUILTIN_CALL_ERRORS.unknownTool
    )
    expect(
      classifyToolResponse(toolError(1, `This MCP connection was not granted access to team T.`)).outcome
    ).toEqual({ denied: true })
    expect(
      classifyToolResponse(toolError(1, `MCP error -32602: Input validation error: Invalid arguments for tool x: …`))
        .outcome
    ).toEqual({ invalid: true })
    expect(classifyToolResponse(undefined).error).toBe(BUILTIN_CALL_ERRORS.noAnswer)
    expect(classifyToolResponse({ jsonrpc: `2.0`, id: 1, error: { code: -32603, message: `x` } }).error).toBe(
      BUILTIN_CALL_ERRORS.rpcError
    )
  })

  it(`one row per tools/call in a batch, matched by id; other methods are not calls`, () => {
    const body = [call(1, `exponential_issues`, { op: `get` }), { jsonrpc: `2.0`, id: 2, method: `tools/list` }, call(3, `exponential_show`)]
    expect(toolCallsOf(body).map((c) => c.tool)).toEqual([`exponential_issues`, `exponential_show`])
    const rows = auditRows(caller, body, [toolError(3, `boom`), ok(1)], 5)
    expect(rows.map((r) => [r.tool, r.status])).toEqual([
      [`exponential_issues`, `ok`],
      [`exponential_show`, `failed`],
    ])
    expect(JSON.stringify(rows)).not.toContain(`boom`)
    expect(JSON.stringify(rows)).not.toContain(`"get"`)
  })
})

describe(`auditTeamId`, () => {
  it(`names the one team a confined access reaches, else none`, () => {
    expect(auditTeamId(FULL_ACCESS)).toBeNull()
    expect(auditTeamId(NO_ACCESS)).toBeNull()
    expect(auditTeamId({ ...NO_ACCESS, visibleTeamIds: new Set([`t1`]) })).toBe(`t1`)
    expect(auditTeamId({ ...NO_ACCESS, visibleTeamIds: new Set([`t1`, `t2`]) })).toBeNull()
  })
})
