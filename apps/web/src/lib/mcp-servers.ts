// EXP-792: presentation + client-side rules for team MCP servers. The rows
// come over tRPC (`mcpServers.list`, server-only: never a shape) and carry
// NON-SECRET config — names, url, header/env NAMES, the auth kind — plus the
// CALLER's own `connection` (the server holds each member's credential,
// encrypted; nothing here ever sees one) and how many members connected.
import type { McpAuth, McpTransport } from "@exp/db-schema/domain"
import type { trpc } from "@/lib/trpc-client"

export type McpServerList = Awaited<
  ReturnType<typeof trpc.mcpServers.list.query>
>
export type McpServerRow = McpServerList[number]
export type McpConnection = McpServerRow[`connection`]
export type McpConnectionStatus = McpConnection[`status`]
export type McpProbeResult = Awaited<
  ReturnType<typeof trpc.mcpServers.probe.mutate>
>

/** Whether the browser may navigate to a `connect` authorize URL: absolute
 * http(s) only. The server already refuses anything else; this is the last
 * gate before `window.location.assign` (never a `javascript:`/`data:` URL). */
export function isNavigableAuthorizeUrl(value: string): boolean {
  try {
    const url = new URL(value)
    return url.protocol === `https:` || url.protocol === `http:`
  } catch {
    return false
  }
}

/** The toast line for an authorize URL the browser refused to open. */
export const UNSAFE_AUTHORIZE_URL_MESSAGE = `The provider's sign-in page is not an http(s) URL.`

export const MCP_TRANSPORT_LABELS: Record<McpTransport, string> = {
  http: `HTTP`,
  stdio: `Command`,
}

export const MCP_AUTH_LABELS: Record<McpAuth, string> = {
  none: `No sign-in`,
  oauth: `OAuth`,
  secret: `API key`,
}

/** Well-known hosted servers the Add dialog offers as one-click tiles. Plain
 * names, no brand marks: the tile is a shortcut for pasting the URL. */
export const MCP_CATALOG: readonly { name: string; url: string }[] = [
  { name: `Linear`, url: `https://mcp.linear.app/mcp` },
  { name: `Notion`, url: `https://mcp.notion.com/mcp` },
  { name: `Sentry`, url: `https://mcp.sentry.dev/mcp` },
  { name: `Stripe`, url: `https://mcp.stripe.com` },
  { name: `Vercel`, url: `https://mcp.vercel.com` },
  { name: `Supabase`, url: `https://mcp.supabase.com/mcp` },
]

/** A run can use the server as the caller stands: nothing to sign in to, or
 * the caller's own credential is usable. Everything else = "Connect first". */
export function mcpServerReady(
  server: Pick<McpServerRow, `connection`>
): boolean {
  const status = server.connection.status
  return status === `connected` || status === `not_needed`
}

/** The picker's muted second line for a server the caller cannot use yet. */
export function mcpNotReadyLabel(
  server: Pick<McpServerRow, `connection`>
): string | null {
  switch (server.connection.status) {
    case `connected`:
    case `not_needed`:
      return null
    case `expired`:
    case `error`:
      return `Reconnect first`
    default:
      return `Connect first`
  }
}

/** Where the server lives, for the row's second line: the URL's host, or the
 * command line of a stdio server. */
export function mcpServerTarget(
  server: Pick<McpServerRow, `transport` | `url` | `command` | `args`>
): string {
  if (server.transport === `http`) {
    if (!server.url) return ``
    try {
      return new URL(server.url).host
    } catch {
      return server.url
    }
  }
  return [server.command, ...server.args].filter(Boolean).join(` `)
}

/** Same URL modulo a trailing slash and case of the host, so a catalog tile
 * can tell it is already on the team. */
export function sameMcpUrl(left: string | null, right: string): boolean {
  if (!left) return false
  const norm = (value: string) => {
    try {
      const url = new URL(value.trim())
      return `${url.protocol}//${url.host.toLowerCase()}${url.pathname.replace(/\/+$/, ``)}`
    } catch {
      return value.trim().replace(/\/+$/, ``)
    }
  }
  return norm(left) === norm(right)
}

/** A lowercase name off a URL's host when the probe offers none:
 * `mcp.linear.app` → `linear`. */
export function nameFromUrl(value: string): string {
  try {
    const parts = new URL(value).hostname.split(`.`).filter(Boolean)
    const meaningful = parts.filter((part) => part !== `mcp` && part !== `www`)
    return (meaningful.length > 1 ? meaningful[meaningful.length - 2] : meaningful[0]) ?? ``
  } catch {
    return ``
  }
}

/** The seed for a launch multiselect: the saved pick for the team when one
 * exists (clamped to rows that still exist), else the `enabledByDefault`
 * rows — either way ONLY servers the caller can use right now: a server they
 * have not connected is never preselected. Order follows the list. */
export function preselectMcpServerIds(
  servers: readonly Pick<McpServerRow, `id` | `enabledByDefault` | `connection`>[],
  saved: readonly string[] | null
): string[] {
  const ready = servers.filter(mcpServerReady)
  if (saved !== null) {
    const known = new Set(ready.map((server) => server.id))
    return saved.filter((id) => known.has(id))
  }
  return ready
    .filter((server) => server.enabledByDefault)
    .map((server) => server.id)
}

/** What the add/edit dialog submits (names as arrays, not chips' text). */
export interface McpServerDraft {
  name: string
  transport: McpTransport
  url: string
  headerNames: string[]
  command: string
  args: string[]
  envNames: string[]
  scopes: string[]
  auth: McpAuth
  enabledByDefault: boolean
}

export const EMPTY_MCP_SERVER_DRAFT: McpServerDraft = {
  name: ``,
  transport: `http`,
  url: ``,
  headerNames: [],
  command: ``,
  args: [],
  envNames: [],
  scopes: [],
  auth: `none`,
  enabledByDefault: true,
}

export function draftFromServer(server: McpServerRow): McpServerDraft {
  return {
    name: server.name,
    transport: server.transport as McpTransport,
    url: server.url ?? ``,
    headerNames: [...server.headerNames],
    command: server.command ?? ``,
    args: [...server.args],
    envNames: [...server.envNames],
    scopes: [...server.scopes],
    auth: server.auth as McpAuth,
    enabledByDefault: server.enabledByDefault,
  }
}

/** The header/env NAME shape the server accepts (`mcpVariableNameSchema`),
 * mirrored so a chip is refused at the field rather than at submit. */
export const MCP_VARIABLE_NAME_RE = /^[A-Za-z_][A-Za-z0-9_-]*$/

/** A URL an HTTP server may live at: https, or http on loopback. Null = ok. */
export function mcpUrlProblem(value: string): string | null {
  const url = value.trim()
  if (url.length === 0) return `Paste the server's URL.`
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return `That URL does not parse.`
  }
  const loopback =
    parsed.hostname === `localhost` || parsed.hostname === `127.0.0.1`
  if (parsed.protocol !== `https:` && !(parsed.protocol === `http:` && loopback)) {
    return `The URL must be https:// (http:// only for localhost).`
  }
  return null
}

/** The cross-field rules `mcpServers.create/update` enforce, client-side, so
 * the dialog can say why before the round trip. Null = submittable. */
export function validateMcpServerDraft(draft: McpServerDraft): string | null {
  if (draft.name.trim().length === 0) return `Give the server a name.`
  const http = draft.transport === `http`
  if (http) {
    const problem = mcpUrlProblem(draft.url)
    if (problem) return problem
  } else if (draft.command.trim().length === 0) {
    return `A command server needs a command.`
  }
  const names = http ? draft.headerNames : draft.envNames
  for (const name of names) {
    if (!MCP_VARIABLE_NAME_RE.test(name)) {
      return `${name} is not a header or variable name.`
    }
  }
  if (draft.auth === `oauth` && !http) {
    return `OAuth sign-in works for HTTP servers only.`
  }
  if (draft.auth === `secret` && names.length !== 1) {
    return http
      ? `An API-key server declares exactly one header name (Advanced): the one that carries the key.`
      : `An API-key server declares exactly one variable name (Advanced): the one that carries the key.`
  }
  return null
}

/** The one-line "how do members get in" the dialog and rows say. */
export function mcpAuthLine(auth: McpAuth): string {
  if (auth === `oauth`) return `Members sign in with their own account (OAuth)`
  if (auth === `secret`) return `Members paste their own API key`
  return `No sign-in needed`
}
