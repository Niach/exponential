// EXP-891: MCP servers a MACHINE connects on its own runs — per device and
// per member, beside the team registry (EXP-792, `lib/mcp-servers.ts`).
//
// The decision (EXP-988 open decision 1): a NEW server-only table,
// `device_mcp_servers`, not a widening of `mcp_servers`. The team registry
// is a shared, owner-authored catalogue with an auth kind, a per-device
// readiness matrix and an OAuth flow behind every row; a device row is the
// opposite — private to one machine and its member, written by the DEVICE
// from what the local claude/codex config already holds (`claude mcp add …`,
// `[mcp_servers.*]` in codex's config.toml), with no credential position at
// all: whatever auth the agent already keeps for that server stays where it
// is. Rows are edited only on the IDE / CLI (import, add, enable, forget);
// the web shows them read-only, grouped per device. Every enabled row is
// appended to every run the device starts (`coding::device_mcp_servers`),
// so there is nothing to pick on the Agent page.
import type { McpTransport } from "@exp/db-schema/domain"

export const deviceMcpServerSources = [`detected`, `manual`] as const
export type DeviceMcpServerSource = (typeof deviceMcpServerSources)[number]

/** The local agent configs a `detected` row can come from. */
export const deviceMcpServerAgents = [`claude`, `codex`] as const
export type DeviceMcpServerAgent = (typeof deviceMcpServerAgents)[number]

export interface DeviceMcpServer {
  id: string
  /** `devices.device_id` (the machine's stable id, not the row id). */
  deviceId: string
  /** The team member who owns the device. */
  userId: string
  name: string
  transport: McpTransport
  /** `http` transport. */
  url?: string | null
  /** `stdio` transport. */
  command?: string | null
  /** `detected` = imported from the local agent config; `manual` = typed. */
  source: DeviceMcpServerSource
  enabled: boolean
}

/** One row as `deviceMcpServers.list` returns it: the contract shape plus
 * what the web needs to group and caption it. */
export interface DeviceMcpServerListRow extends DeviceMcpServer {
  /** The `devices` row id. */
  deviceRowId: string
  deviceLabel: string
  /** The device owner's display name — the caller's own name on own rows. */
  ownerName: string
  /** `stdio` arguments. */
  args: string[]
  /** The local config a detected row was imported from; null when typed. */
  agent: DeviceMcpServerAgent | null
  updatedAt: string
}

/** The per-machine set the device pushes with `deviceMcpServers.sync` —
 * the whole list, replacing what the server held for that device. */
export interface DeviceMcpServerInput {
  name: string
  transport: McpTransport
  url?: string | null
  command?: string | null
  args?: string[]
  source: DeviceMcpServerSource
  agent?: DeviceMcpServerAgent | null
  enabled: boolean
}

/** Rows one device may hold (a hand-written config never lists more). */
export const MAX_DEVICE_MCP_SERVERS = 64
export const MAX_DEVICE_MCP_SERVER_NAME = 64

/** The web hand-off line: where a row gets added, imported or switched. */
export const DEVICE_MCP_SETUP_HINT = `Servers each machine connects on its own runs. Import or add them in the desktop app under Settings › MCP servers, or with the CLI: exponential mcp import, exponential mcp add.`

/** `https://mcp.linear.app/mcp` for http, the command line for stdio. */
export function deviceMcpServerTarget(
  row: Pick<DeviceMcpServer, `transport` | `url` | `command`> & {
    args?: string[] | null
  }
): string {
  if (row.transport === `http`) return row.url ?? ``
  return [row.command, ...(row.args ?? [])].filter(Boolean).join(` `)
}

/** The provenance chip: `Detected from Claude Code`, `Detected from Codex`,
 * `Added on the device`. */
export function deviceMcpServerSourceLabel(
  row: Pick<DeviceMcpServer, `source`> & {
    agent?: DeviceMcpServerAgent | string | null
  }
): string {
  if (row.source === `detected`) {
    if (row.agent === `claude`) return `Detected from Claude Code`
    if (row.agent === `codex`) return `Detected from Codex`
    return `Detected on the device`
  }
  return `Added on the device`
}

export interface DeviceMcpServerGroup {
  deviceId: string
  deviceRowId: string
  deviceLabel: string
  userId: string
  ownerName: string
  /** Name order, case-insensitive. */
  servers: DeviceMcpServerListRow[]
}

/** Rows grouped per device (the settings pane's one band per machine):
 * the caller's own machines first, then teammates' shared ones, each set
 * alphabetical by label; rows inside a group by name. */
export function groupDeviceMcpServers(
  rows: readonly DeviceMcpServerListRow[],
  currentUserId: string
): DeviceMcpServerGroup[] {
  const byDevice = new Map<string, DeviceMcpServerGroup>()
  for (const row of rows) {
    let group = byDevice.get(row.deviceRowId)
    if (!group) {
      group = {
        deviceId: row.deviceId,
        deviceRowId: row.deviceRowId,
        deviceLabel: row.deviceLabel,
        userId: row.userId,
        ownerName: row.ownerName,
        servers: [],
      }
      byDevice.set(row.deviceRowId, group)
    }
    group.servers.push(row)
  }
  const byName = (a: { name: string }, b: { name: string }) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: `base` })
  const groups = [...byDevice.values()]
  for (const group of groups) group.servers.sort(byName)
  groups.sort((a, b) => {
    const mineA = a.userId === currentUserId ? 0 : 1
    const mineB = b.userId === currentUserId ? 0 : 1
    if (mineA !== mineB) return mineA - mineB
    return (a.deviceLabel || a.deviceId).localeCompare(
      b.deviceLabel || b.deviceId,
      undefined,
      { sensitivity: `base` }
    )
  })
  return groups
}

/** The row rules `deviceMcpServers.sync` enforces, shared with the input
 * schema so a refusal names the field: an http row needs a URL (https, or
 * http on the machine's own loopback — a local dev MCP never leaves it), a
 * stdio row a command, and no row may fold to the launcher's reserved
 * `exponential` config key. Returns null for a valid row. */
export function validateDeviceMcpServerInput(
  input: DeviceMcpServerInput
): string | null {
  const name = input.name.trim()
  if (name.length === 0) return `A server needs a name`
  if (name.length > MAX_DEVICE_MCP_SERVER_NAME) {
    return `Server names are at most ${MAX_DEVICE_MCP_SERVER_NAME} characters`
  }
  if (configKey(name) === `exponential`) return `The name ${name} is reserved`
  if (input.transport === `http`) {
    const url = (input.url ?? ``).trim()
    if (url.length === 0) return `${name}: an http server needs a URL`
    let parsed: URL
    try {
      parsed = new URL(url)
    } catch {
      return `${name}: that URL does not parse`
    }
    const loopback =
      parsed.hostname === `localhost` || parsed.hostname === `127.0.0.1`
    if (
      parsed.protocol !== `https:` &&
      !(parsed.protocol === `http:` && loopback)
    ) {
      return `${name}: the URL must be https:// (http:// only for localhost)`
    }
  } else if ((input.command ?? ``).trim().length === 0) {
    return `${name}: a stdio server needs a command`
  }
  return null
}

/** The config key a name becomes in the rendered agent config (the desktop
 * `McpServerWire::config_key` twin): lowercase ASCII alphanumerics, the rest
 * folded to `_`, never empty. Two rows folding to one key would shadow each
 * other, so `sync` refuses the second. */
export function configKey(name: string): string {
  const key = name
    .trim()
    .split(``)
    .map((c) => (/[A-Za-z0-9]/.test(c) ? c.toLowerCase() : `_`))
    .join(``)
  return key.length === 0 ? `server` : key
}
