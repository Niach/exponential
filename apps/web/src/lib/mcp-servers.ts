// EXP-792: presentation + client-side rules for team MCP servers. The rows
// come over tRPC (`mcpServers.list`, server-only: never a shape) and carry
// NON-SECRET config only — names, url, header/env NAMES, the auth kind — plus
// a per-device readiness matrix the devices report from their secret stores.
// Nothing here ever sees a credential.
import type { McpAuth, McpTransport } from "@exp/db-schema/domain"
import type { trpc } from "@/lib/trpc-client"

export type McpServerList = Awaited<
  ReturnType<typeof trpc.mcpServers.list.query>
>
export type McpServerRow = McpServerList[number]
export type McpServerReadinessEntry = McpServerRow[`readiness`][number]

export const MCP_TRANSPORT_LABELS: Record<McpTransport, string> = {
  http: `HTTP`,
  stdio: `Command`,
}

export const MCP_AUTH_LABELS: Record<McpAuth, string> = {
  none: `No auth`,
  oauth: `OAuth`,
  secret: `Secret`,
}

/** The readiness row a device reported for `server`, matched on the steer
 * `deviceId` (the devices row's `device_id` column, what every picker keys
 * on), or null when the device never reported for it. */
export function readinessFor(
  server: Pick<McpServerRow, `readiness`>,
  deviceId: string | null | undefined
): McpServerReadinessEntry | null {
  if (!deviceId) return null
  return server.readiness.find((entry) => entry.deviceId === deviceId) ?? null
}

/** Whether an OAuth token the device holds is already past its expiry — the
 * device refreshes on its heartbeat, so this is only ever a beat late. */
export function readinessExpired(
  entry: Pick<McpServerReadinessEntry, `expiresAt`>,
  now: Date
): boolean {
  if (!entry.expiresAt) return false
  const at = new Date(entry.expiresAt).getTime()
  return !Number.isNaN(at) && at <= now.getTime()
}

/** `Ready`, `Signed in until 14:05`, or the error the device reported. A
 * missing report reads as the per-auth "nothing on the device yet" line. */
export function readinessLabel(
  server: Pick<McpServerRow, `auth` | `transport`>,
  entry: McpServerReadinessEntry | null,
  now: Date
): string {
  if (!entry) {
    if (server.auth === `oauth`) return `Not signed in`
    if (server.auth === `secret`) return `Not set`
    return `Not checked yet`
  }
  if (entry.ready) {
    if (entry.expiresAt) {
      if (readinessExpired(entry, now)) return `Expired`
      return `Signed in until ${formatUntil(entry.expiresAt, now)}`
    }
    return `Ready`
  }
  return entry.error || (server.auth === `oauth` ? `Not signed in` : `Not set`)
}

/** `14:05` today, `Tue 14:05` within the week, else `12 Sep`. Locale-formatted
 * so the hour reads the way the person's clock does. */
function formatUntil(iso: string, now: Date): string {
  const at = new Date(iso)
  if (Number.isNaN(at.getTime())) return iso
  const time = at.toLocaleTimeString(undefined, {
    hour: `2-digit`,
    minute: `2-digit`,
  })
  const sameDay = at.toDateString() === now.toDateString()
  if (sameDay) return time
  const withinWeek = at.getTime() - now.getTime() < 7 * 86_400_000
  if (withinWeek) {
    return `${at.toLocaleDateString(undefined, { weekday: `short` })} ${time}`
  }
  return at.toLocaleDateString(undefined, { day: `numeric`, month: `short` })
}

/** A server the device can connect right now: `none` needs nothing; the other
 * kinds need a ready, unexpired report from THAT device. */
export function serverReadyOn(
  server: Pick<McpServerRow, `auth` | `readiness`>,
  deviceId: string | null | undefined,
  now: Date
): boolean {
  if (server.auth === `none`) return true
  const entry = readinessFor(server, deviceId)
  if (!entry || !entry.ready) return false
  return !readinessExpired(entry, now)
}

/** The tooltip reason a launch multiselect greys a server out with, or null
 * when the chosen device is ready for it. */
export function serverBlockReason(
  server: Pick<McpServerRow, `auth` | `readiness` | `transport`>,
  device: { deviceId: string; deviceLabel: string } | null | undefined,
  now: Date
): string | null {
  if (server.auth === `none`) return null
  if (!device) return null
  if (serverReadyOn(server, device.deviceId, now)) return null
  const label = device.deviceLabel || device.deviceId
  const entry = readinessFor(server, device.deviceId)
  if (entry && !entry.ready && entry.error) return `${entry.error} on ${label}`
  if (server.auth === `oauth`) return `Not signed in on ${label}`
  return `No value set on ${label}`
}

/** The one line telling a person how a `secret` value gets onto a machine:
 * the desktop pane or the CLI. `name` is the ONE declared header/env name. */
export function secretSetupHint(
  server: Pick<McpServerRow, `name` | `transport` | `headerNames` | `envNames`>
): string {
  const name =
    (server.transport === `http` ? server.headerNames : server.envNames)[0] ??
    `<NAME>`
  return `Set on the device: Settings › MCP servers in the desktop app, or run \`exponential mcp set-secret ${server.name} ${name}\`.`
}

/** The seed for a launch multiselect: the saved pick for the team when one
 * exists (clamped to rows that still exist), else the `enabledByDefault`
 * rows. Order follows the list (alphabetical from the server). */
export function preselectMcpServerIds(
  servers: readonly Pick<McpServerRow, `id` | `enabledByDefault`>[],
  saved: readonly string[] | null
): string[] {
  if (saved !== null) {
    const known = new Set(servers.map((server) => server.id))
    return saved.filter((id) => known.has(id))
  }
  return servers
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
  enabledByDefault: false,
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

/** The cross-field rules `mcpServers.create/update` enforce, client-side, so
 * the dialog can say why before the round trip. Null = submittable. */
export function validateMcpServerDraft(draft: McpServerDraft): string | null {
  if (draft.name.trim().length === 0) return `Give the server a name.`
  const http = draft.transport === `http`
  if (http) {
    const url = draft.url.trim()
    if (url.length === 0) return `An HTTP server needs a URL.`
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
      ? `A secret server declares exactly one header name: the one that carries the secret.`
      : `A secret server declares exactly one variable name: the one that carries the secret.`
  }
  return null
}
