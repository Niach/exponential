// #EXP-1491: the `mcp_calls` audit (@exp/emcp `fixtures/audit.json`). The
// gate (gate.ts) reads every `tools/call` out of the JSON-RPC request and its
// answer, so ANY tool provider behind it is audited the same way without
// touching its handlers. Written after the call, never in its way: a failed
// insert is logged and the call has already answered. The arguments are
// never stored, only their canonical hash; `error` is a short code, never
// the message.
import { lte } from "drizzle-orm"
import { argsHash, AUDIT_RETENTION_DAYS, callStatus, type CallOutcome, type CallStatus } from "@exp/emcp/audit"
import { DELETE_BURST_AUDIT_ERROR, destructiveTarget } from "@exp/emcp/deletes"
import { db } from "@/db/connection"
import { mcpCalls } from "@/db/schema"
import { reportSchedulerRun } from "@/lib/metrics/registry"

/** One `tools/call` the client sent. */
export interface ToolCallRequest {
  id: string | number | null
  tool: string
  args: unknown
}

/** The `tools/call` messages of a JSON-RPC body (one message or a batch).
 * Anything else (initialize, tools/list, notifications) is not a call. */
export function toolCallsOf(body: unknown): ToolCallRequest[] {
  const messages = Array.isArray(body) ? body : [body]
  const calls: ToolCallRequest[] = []
  for (const message of messages) {
    if (!message || typeof message !== `object`) continue
    const m = message as { method?: unknown; id?: unknown; params?: unknown }
    if (m.method !== `tools/call`) continue
    const params = (m.params && typeof m.params === `object` ? m.params : {}) as {
      name?: unknown
      arguments?: unknown
    }
    const tool = typeof params.name === `string` ? params.name : ``
    if (!tool) continue
    const id = typeof m.id === `string` || typeof m.id === `number` ? m.id : null
    calls.push({ id, tool: tool.slice(0, 128), args: params.arguments })
  }
  return calls
}

/** #NEW-147: one JSON-RPC REQUEST the client sent (any method; a
 * notification has no id and is not audited). `tool` = what it names: the
 * tool or prompt (`params.name`), the uri, else the method (never a taskId). */
export interface McpRequestEntry {
  id: string | number | null
  method: string
  tool: string
  args: unknown
}

/** Every request of a JSON-RPC body (one message or a batch), in order. */
export function requestsOf(body: unknown): McpRequestEntry[] {
  const out: McpRequestEntry[] = []
  for (const message of Array.isArray(body) ? body : [body]) {
    if (!message || typeof message !== `object`) continue
    const m = message as { method?: unknown; id?: unknown; params?: unknown }
    if (typeof m.method !== `string` || !m.method) continue
    if (typeof m.id !== `string` && typeof m.id !== `number`) continue
    const params = (m.params && typeof m.params === `object` ? m.params : {}) as Record<string, unknown>
    const named = [params.name, params.uri].find((value) => typeof value === `string` && value !== ``) as string | undefined
    if (m.method === `tools/call` && !named) continue
    out.push({
      id: m.id,
      method: m.method.slice(0, 64),
      tool: (named ?? m.method).slice(0, 128),
      args: m.method === `tools/call` || m.method === `prompts/get` ? params.arguments : undefined,
    })
  }
  return out
}

/** A non-tool request's outcome: an error answer, else ok. */
function classifyRequestResponse(response: unknown): { outcome: CallOutcome; error: string | null } {
  if (!response || typeof response !== `object`) return { outcome: { error: true }, error: BUILTIN_CALL_ERRORS.noAnswer }
  return (response as { error?: unknown }).error ? { outcome: { error: true }, error: BUILTIN_CALL_ERRORS.rpcError } : { outcome: {}, error: null }
}

/** The responses of a JSON-RPC answer, by id. */
export function responsesById(body: unknown): Map<string | number, unknown> {
  const out = new Map<string | number, unknown>()
  for (const message of Array.isArray(body) ? body : [body]) {
    if (!message || typeof message !== `object`) continue
    const id = (message as { id?: unknown }).id
    if (typeof id === `string` || typeof id === `number`) out.set(id, message)
  }
  return out
}

/** The error codes of a built-in call (never a message). */
export const BUILTIN_CALL_ERRORS = {
  unknownTool: `unknownTool`,
  notGranted: `notGranted`,
  invalidArguments: `invalidArguments`,
  toolError: `toolError`,
  rpcError: `rpcError`,
  noAnswer: `noAnswer`,
  /** #EXP-1798: refused by the delete burst guard (deletes.json). */
  deleteBurst: DELETE_BURST_AUDIT_ERROR as `deleteBurst`,
} as const

// The SDK answers an unknown or disabled tool and failed input validation as
// an error RESULT (McpServer.createToolError), so the words are the signal.
const UNKNOWN_TOOL = /^MCP error -32602: Tool \S+ (not found|disabled)$/
const INVALID_INPUT = /^MCP error -32602: Input validation error/
// scope.ts's denials all keep this wording (clients key on it too).
const NOT_GRANTED = /not granted access/
// #EXP-1798: the burst guard's refusal (deletes.json `errors.burst`).
const DELETE_BURST = /^Refused: more than \d+ deletes within/

function errorText(result: { content?: unknown }): string {
  const content = Array.isArray(result.content) ? result.content : []
  const first = content.find((c) => c && typeof c === `object` && (c as { type?: unknown }).type === `text`) as
    | { text?: unknown }
    | undefined
  return typeof first?.text === `string` ? first.text : ``
}

/** A call's outcome + short error code from its JSON-RPC response. */
export function classifyToolResponse(response: unknown): { outcome: CallOutcome; error: string | null } {
  if (!response || typeof response !== `object`) {
    return { outcome: { error: true }, error: BUILTIN_CALL_ERRORS.noAnswer }
  }
  const r = response as { result?: unknown; error?: unknown }
  if (r.error) return { outcome: { error: true }, error: BUILTIN_CALL_ERRORS.rpcError }
  const result = (r.result && typeof r.result === `object` ? r.result : {}) as {
    isError?: unknown
    content?: unknown
  }
  if (result.isError !== true) return { outcome: {}, error: null }
  const text = errorText(result)
  if (UNKNOWN_TOOL.test(text)) return { outcome: { denied: true }, error: BUILTIN_CALL_ERRORS.unknownTool }
  if (INVALID_INPUT.test(text)) return { outcome: { invalid: true }, error: BUILTIN_CALL_ERRORS.invalidArguments }
  if (NOT_GRANTED.test(text)) return { outcome: { denied: true }, error: BUILTIN_CALL_ERRORS.notGranted }
  if (DELETE_BURST.test(text)) return { outcome: { error: true }, error: BUILTIN_CALL_ERRORS.deleteBurst }
  return { outcome: { error: true }, error: BUILTIN_CALL_ERRORS.toolError }
}

/** Who made the calls of one request. */
export interface AuditCaller {
  userId: string
  clientId: string | null
  teamId: string | null
  appId: string | null
  /** #EXP-1798: the caller's own verified session, if any. */
  sessionId?: string | null
}

export interface McpCallRow {
  teamId: string | null
  userId: string
  clientId: string | null
  appId: string | null
  tool: string
  /** #NEW-147: the JSON-RPC method (`tools/call`, `skills/list`, `tasks/get`…). */
  method: string
  status: CallStatus
  error: string | null
  latencyMs: number
  argsHash: string
  /** #EXP-1798: the caller's verified session. */
  sessionId: string | null
  /** #EXP-1798: `<op>:<id>` of a destructive call (deletes.json), else null. */
  target: string | null
}

/** #VAPP-181: an endpoint's own reading of its calls: the app whose tool
 * a name runs (`/api/mcp/apps`) and how its answers classify. */
export interface AuditOptions {
  appIdOf?: (tool: string) => string | null
  classify?: (response: unknown) => { outcome: CallOutcome; error: string | null }
  /** #NEW-147: only `tools/call` rows (`/api/mcp/apps`; audit.json audits every method of `/api/mcp`). */
  toolCallsOnly?: boolean
}

/** The requests of `body` an endpoint audits. */
export function auditedRequests(body: unknown, options: Pick<AuditOptions, `toolCallsOnly`> = {}): McpRequestEntry[] {
  const requests = requestsOf(body)
  return options.toolCallsOnly ? requests.filter((call) => call.method === `tools/call`) : requests
}

/** The audit rows of one request: #NEW-147 one per JSON-RPC request (every
 * method; a `tools/call` classified by its tool answer), in request order. */
export function auditRows(
  caller: AuditCaller,
  requestBody: unknown,
  responseBody: unknown,
  latencyMs: number,
  options: AuditOptions = {}
): McpCallRow[] {
  const responses = responsesById(responseBody)
  const classify = options.classify ?? classifyToolResponse
  return auditedRequests(requestBody, options).map((call) => {
    const tool = call.method === `tools/call`
    const { outcome, error } = (tool ? classify : classifyRequestResponse)(call.id === null ? null : responses.get(call.id))
    return {
      teamId: caller.teamId,
      userId: caller.userId,
      clientId: caller.clientId,
      appId: (tool ? options.appIdOf?.(call.tool) : null) ?? caller.appId,
      tool: call.tool,
      method: call.method,
      status: callStatus(outcome),
      error,
      latencyMs: Math.max(0, Math.floor(latencyMs)),
      argsHash: argsHash(call.args),
      sessionId: caller.sessionId ?? null,
      target: tool ? destructiveTarget(call.tool, call.args) : null,
    }
  })
}

/** Insert the rows; never throws. */
export async function recordMcpCalls(rows: McpCallRow[]): Promise<void> {
  if (rows.length === 0) return
  try {
    await db.insert(mcpCalls).values(rows)
  } catch (err) {
    console.error(`[emcp-audit] could not record ${rows.length} call(s):`, err)
  }
}

// ── Retention ───────────────────────────────────────────────────────────

export const AUDIT_RETENTION_MS = AUDIT_RETENTION_DAYS * 86_400_000

/** One pass: delete the rows past retention (audit.json `auditExpired`). */
export async function runMcpAuditSweep(
  now: Date = new Date(),
  database: typeof db | Parameters<Parameters<typeof db.transaction>[0]>[0] = db
): Promise<{ callsDeleted: number }> {
  const cutoff = new Date(now.getTime() - AUDIT_RETENTION_MS)
  const deleted = await database
    .delete(mcpCalls)
    .where(lte(mcpCalls.createdAt, cutoff))
    .returning({ id: mcpCalls.id })
  return { callsDeleted: deleted.length }
}

export async function sweepMcpAudit(): Promise<void> {
  const startMs = performance.now()
  try {
    const result = await runMcpAuditSweep()
    reportSchedulerRun(`emcp-audit-sweep`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.callsDeleted} deleted`,
    })
  } catch (err) {
    reportSchedulerRun(`emcp-audit-sweep`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[emcp-audit] sweep failed:`, err)
  }
}
