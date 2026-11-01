// A minimal Streamable HTTP MCP client for the server's two checks: `probe`
// (what does this URL need before an owner adds it?) and `test` (does the
// member's credential open a session and list tools?). JSON-RPC over POST
// with `accept: application/json, text/event-stream`; an SSE answer is read
// until the message with our id arrives. The `Mcp-Session-Id` the
// initialize answer carries rides every later request. Never logs a header.
import { McpHttpError, mcpFetch } from "@/lib/emcp/upstream/net"
import { discover } from "@/lib/emcp/upstream/oauth-client"
import { readJsonRpcAnswer, type JsonRpcMessage } from "@/lib/emcp/upstream/json-rpc-reader"

const PROTOCOL_VERSION = `2025-06-18`

interface Session {
  url: string
  headers: Record<string, string>
  sessionId: string | null
  protocolVersion: string
}

async function rpc(
  session: Session,
  body: Record<string, unknown>
): Promise<{ response: Response; message: JsonRpcMessage | null }> {
  const headers: Record<string, string> = {
    ...session.headers,
    "content-type": `application/json`,
    accept: `application/json, text/event-stream`,
    "mcp-protocol-version": session.protocolVersion,
  }
  if (session.sessionId) headers[`mcp-session-id`] = session.sessionId
  const response = await mcpFetch(session.url, {
    method: `POST`,
    headers,
    body: JSON.stringify(body),
  })
  const id = typeof body.id === `number` ? body.id : null
  if (!response.ok || id === null) {
    await response.body?.cancel().catch(() => undefined)
    return { response, message: null }
  }
  return { response, message: await readJsonRpcAnswer(response, id) }
}

function statusError(response: Response): string {
  if (response.status === 401 || response.status === 403) {
    return `the server refused the credential (HTTP ${response.status}); reconnect`
  }
  return `the server answered HTTP ${response.status}`
}

/** initialize → notifications/initialized → tools/list with `headers`. */
export async function testMcpServer(
  url: string,
  headers: Record<string, string>
): Promise<{ ok: boolean; tools: number | null; error: string | null }> {
  const session: Session = { url, headers, sessionId: null, protocolVersion: PROTOCOL_VERSION }
  try {
    const init = await rpc(session, {
      jsonrpc: `2.0`,
      id: 1,
      method: `initialize`,
      params: {
        protocolVersion: PROTOCOL_VERSION,
        capabilities: {},
        clientInfo: { name: `Exponential`, version: `1.0.0` },
      },
    })
    if (!init.response.ok) {
      return { ok: false, tools: null, error: statusError(init.response) }
    }
    if (!init.message?.result) {
      return {
        ok: false,
        tools: null,
        error: init.message?.error?.message?.slice(0, 300) ?? `no initialize answer`,
      }
    }
    session.sessionId = init.response.headers.get(`mcp-session-id`)
    const negotiated = init.message.result.protocolVersion
    if (typeof negotiated === `string`) session.protocolVersion = negotiated
    await rpc(session, { jsonrpc: `2.0`, method: `notifications/initialized` })
    const list = await rpc(session, { jsonrpc: `2.0`, id: 2, method: `tools/list`, params: {} })
    if (session.sessionId) {
      // Best effort: let the server drop the session now.
      void mcpFetch(url, {
        method: `DELETE`,
        headers: { ...headers, "mcp-session-id": session.sessionId },
      }).catch(() => undefined)
    }
    if (!list.response.ok) {
      return { ok: false, tools: null, error: statusError(list.response) }
    }
    const tools = list.message?.result?.tools
    if (!Array.isArray(tools)) {
      return {
        ok: false,
        tools: null,
        error: list.message?.error?.message?.slice(0, 300) ?? `no tools/list answer`,
      }
    }
    return { ok: true, tools: tools.length, error: null }
  } catch (e) {
    return {
      ok: false,
      tools: null,
      error: e instanceof McpHttpError ? e.message : `the connection failed`,
    }
  }
}

/** A lowercase name from the URL's host: `mcp.linear.app` → `linear`. */
export function suggestedServerName(url: string): string {
  let host: string
  try {
    host = new URL(url).hostname.toLowerCase()
  } catch {
    return `server`
  }
  const labels = host.split(`.`).filter(Boolean)
  while (labels.length > 2 && [`mcp`, `api`, `www`, `app`].includes(labels[0]!)) {
    labels.shift()
  }
  const pick = labels.length >= 2 ? labels[labels.length - 2]! : (labels[0] ?? `server`)
  return pick.replace(/[^a-z0-9-]/g, ``).slice(0, 64) || `server`
}

export interface ProbeResult {
  url: string
  suggestedName: string
  auth: `oauth` | `none`
  reachable: boolean
  error: string | null
  scopes: string[]
}

/** What an http URL needs: an unauthenticated initialize that succeeds =
 * `none`; a 401/403 with OAuth metadata = `oauth` (+ the advertised scopes). */
export async function probeMcpServer(rawUrl: string): Promise<ProbeResult> {
  const url = rawUrl.trim()
  const result: ProbeResult = {
    url,
    suggestedName: suggestedServerName(url),
    auth: `none`,
    reachable: false,
    error: null,
    scopes: [],
  }
  const session: Session = { url, headers: {}, sessionId: null, protocolVersion: PROTOCOL_VERSION }
  let status: number
  try {
    const init = await rpc(session, {
      jsonrpc: `2.0`,
      id: 1,
      method: `initialize`,
      params: {
        protocolVersion: PROTOCOL_VERSION,
        capabilities: {},
        clientInfo: { name: `Exponential`, version: `1.0.0` },
      },
    })
    status = init.response.status
  } catch (e) {
    result.error = e instanceof McpHttpError ? e.message : `the connection failed`
    return result
  }
  result.reachable = true
  if (status >= 200 && status < 300) return result
  if (status !== 401 && status !== 403) {
    result.error = `the server answered HTTP ${status}`
    return result
  }
  try {
    const discovery = await discover(url)
    result.auth = `oauth`
    result.scopes = discovery.scopesSupported.slice(0, 16)
  } catch {
    result.error = `the server needs authentication but advertises no OAuth sign-in; add it with a secret header instead`
  }
  return result
}

/** #VAPP-181: one upstream tool call, as an app tool's broker makes it. */
export type UpstreamCall =
  | { ok: true; annotations: Record<string, unknown> | null; result: Record<string, unknown> }
  /** `refused` = the server refused the credential (reconnect), `noTool` =
   *  it lists no such tool, `failed` = anything else (unreachable, an
   *  error answer, a timeout). */
  | { ok: false; reason: `refused` | `noTool` | `failed`; error: string }

const TOOL_LIST_PAGES = 10

/** initialize → notifications/initialized → tools/list (the tool's live
 * annotations, which `check` may veto before anything runs) → tools/call. */
export async function callUpstreamTool(
  url: string,
  headers: Record<string, string>,
  tool: string,
  args: Record<string, unknown>,
  check: (annotations: Record<string, unknown> | null) => string | null
): Promise<UpstreamCall | { ok: false; reason: `vetoed`; error: string }> {
  const session: Session = { url, headers, sessionId: null, protocolVersion: PROTOCOL_VERSION }
  const refusedOrFailed = (response: Response): UpstreamCall =>
    response.status === 401 || response.status === 403
      ? { ok: false, reason: `refused`, error: statusError(response) }
      : { ok: false, reason: `failed`, error: statusError(response) }
  try {
    const init = await rpc(session, {
      jsonrpc: `2.0`,
      id: 1,
      method: `initialize`,
      params: { protocolVersion: PROTOCOL_VERSION, capabilities: {}, clientInfo: { name: `Exponential`, version: `1.0.0` } },
    })
    if (!init.response.ok) return refusedOrFailed(init.response)
    if (!init.message?.result) return { ok: false, reason: `failed`, error: init.message?.error?.message?.slice(0, 300) ?? `no initialize answer` }
    session.sessionId = init.response.headers.get(`mcp-session-id`)
    const negotiated = init.message.result.protocolVersion
    if (typeof negotiated === `string`) session.protocolVersion = negotiated
    await rpc(session, { jsonrpc: `2.0`, method: `notifications/initialized` })
    let found: Record<string, unknown> | null = null
    let cursor: string | undefined
    for (let page = 0; page < TOOL_LIST_PAGES && !found; page++) {
      const list = await rpc(session, { jsonrpc: `2.0`, id: 2 + page, method: `tools/list`, params: cursor ? { cursor } : {} })
      if (!list.response.ok) return refusedOrFailed(list.response)
      const tools = list.message?.result?.tools
      if (!Array.isArray(tools)) return { ok: false, reason: `failed`, error: list.message?.error?.message?.slice(0, 300) ?? `no tools/list answer` }
      found = (tools.find((t) => t && typeof t === `object` && (t as { name?: unknown }).name === tool) as Record<string, unknown> | undefined) ?? null
      const next = list.message?.result?.nextCursor
      if (typeof next !== `string` || !next) break
      cursor = next
    }
    if (!found) return { ok: false, reason: `noTool`, error: `the server lists no tool ${tool}` }
    const annotations = found.annotations && typeof found.annotations === `object` ? (found.annotations as Record<string, unknown>) : null
    const vetoed = check(annotations)
    if (vetoed) return { ok: false, reason: `vetoed`, error: vetoed }
    const call = await rpc(session, { jsonrpc: `2.0`, id: 100, method: `tools/call`, params: { name: tool, arguments: args } })
    if (!call.response.ok) return refusedOrFailed(call.response)
    if (!call.message?.result) return { ok: false, reason: `failed`, error: call.message?.error?.message?.slice(0, 300) ?? `no tools/call answer` }
    return { ok: true, annotations, result: call.message.result }
  } catch (e) {
    return { ok: false, reason: `failed`, error: e instanceof McpHttpError ? e.message : `the connection failed` }
  } finally {
    if (session.sessionId) {
      void mcpFetch(url, { method: `DELETE`, headers: { ...headers, "mcp-session-id": session.sessionId } }).catch(() => undefined)
    }
  }
}
