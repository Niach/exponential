// A minimal Streamable HTTP MCP client for the server's two checks: `probe`
// (what does this URL need before an owner adds it?) and `test` (does the
// member's credential open a session and list tools?). JSON-RPC over POST
// with `accept: application/json, text/event-stream`; an SSE answer is read
// until the message with our id arrives. The `Mcp-Session-Id` the
// initialize answer carries rides every later request. Never logs a header.
import { McpHttpError, mcpFetch } from "@/lib/mcp-oauth/net"
import { discover } from "@/lib/mcp-oauth/oauth-client"

const PROTOCOL_VERSION = `2025-06-18`

type JsonRpcMessage = {
  id?: number | string | null
  result?: Record<string, unknown>
  error?: { message?: string }
}

/** Pull the JSON-RPC message with `id` out of an SSE body, reading only as
 * far as it (a server may keep the stream open after answering). */
async function readSse(response: Response, id: number): Promise<JsonRpcMessage | null> {
  const reader = response.body?.getReader()
  if (!reader) return null
  const decoder = new TextDecoder()
  let buffer = ``
  let data: string[] = []
  try {
    for (;;) {
      const { value, done } = await reader.read()
      buffer += decoder.decode(value ?? new Uint8Array(), { stream: !done })
      let newline: number
      while ((newline = buffer.indexOf(`\n`)) >= 0) {
        const line = buffer.slice(0, newline).replace(/\r$/, ``)
        buffer = buffer.slice(newline + 1)
        if (line.startsWith(`data:`)) {
          data.push(line.slice(5).replace(/^ /, ``))
        } else if (line === `` && data.length > 0) {
          const message = parseMessage(data.join(`\n`), id)
          data = []
          if (message) return message
        }
      }
      if (done) {
        return data.length > 0 ? parseMessage(data.join(`\n`), id) : null
      }
    }
  } finally {
    reader.cancel().catch(() => undefined)
  }
}

function parseMessage(text: string, id: number): JsonRpcMessage | null {
  try {
    const parsed: unknown = JSON.parse(text)
    const messages = Array.isArray(parsed) ? parsed : [parsed]
    for (const message of messages) {
      if (message && typeof message === `object` && (message as JsonRpcMessage).id === id) {
        return message as JsonRpcMessage
      }
    }
  } catch {
    // not JSON (a comment / keep-alive): keep reading
  }
  return null
}

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
  const type = response.headers.get(`content-type`) ?? ``
  if (type.includes(`text/event-stream`)) {
    return { response, message: await readSse(response, id) }
  }
  const text = await response.text()
  return { response, message: parseMessage(text, id) }
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
