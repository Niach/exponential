// #VAPP-185: Settings › MCP, the pure reference every client mirrors
// (fixtures/settings-mcp.json is the spec; desktop `settings/mcp.rs`, iOS
// `McpSettings.swift`, Android `McpSettings.kt` replay its cases).

import fixture from "../fixtures/settings-mcp.json" with { type: "json" }

export const SETTINGS_MCP = fixture

export type McpScope = `mine` | `everyone`
export type McpPolicyLevel = `none` | `read` | `write`

/** A server row's "Used by …" caption (null = no app uses it). */
export function usedByCaption(apps: readonly string[]): string | null {
  if (apps.length === 0) return null
  const sorted = [...apps].sort((a, b) => a.localeCompare(b, `en`, { sensitivity: `base` }))
  return fixture.usedBy.replace(`{apps}`, sorted.join(`, `))
}

/** #VAPP-239: a device-routed server row's "Runs on …" caption part (null
 *  = the server broker calls it). */
export function runsOnCaption(device: string | null | undefined): string | null {
  return device ? fixture.runsOn.replace(`{device}`, device) : null
}

export interface McpClientCaptionInput {
  userName: string
  kinds: readonly string[]
  appsAccess: string | null
  /** A relative time ("2h ago"), null = never. */
  lastUsed: string | null
}

/** A client row's caption: [member ·] kinds (apps access) · last used. */
export function clientCaption(scope: McpScope, client: McpClientCaptionInput): string {
  const kinds = [`builtin`, `apps`]
    .filter((k) => client.kinds.includes(k))
    .map((k) => {
      const label = fixture.kindLabels[k as `builtin` | `apps`]
      return k === `apps` && client.appsAccess ? `${label} (${fixture.accessLabels[client.appsAccess as `read` | `full`]})` : label
    })
    .join(`, `)
  return [scope === `everyone` ? client.userName : null, kinds, client.lastUsed ?? fixture.never].filter(Boolean).join(` · `)
}

export interface McpActivityCaptionInput {
  appName: string | null
  userName: string
  clientName: string | null
  when: string
}

/** An activity row's caption: app · [member ·] client · when. */
export function activityCaption(owner: boolean, call: McpActivityCaptionInput): string {
  return [call.appName ?? fixture.builtinApp, owner ? call.userName : null, call.clientName ?? fixture.firstParty, call.when]
    .filter(Boolean)
    .join(` · `)
}

/** "1 tool" | "3 tools". */
export function toolCountLabel(n: number): string {
  return (n === 1 ? fixture.toolCount.one : fixture.toolCount.other).replace(`{n}`, String(n))
}

/** The label of a policy level. */
export function levelLabel(level: McpPolicyLevel): string {
  return fixture.levels.find((l) => l.value === level)?.label ?? level
}

// EXP-1657: the BUILT-IN section = one band per contract tool group
// (`contract.expToolDisplay.groups`, labels from there), each listing its
// rows in contract order; retired alias rows (`aliasOf`) are never listed.
import contractJson from "../contract.json" with { type: "json" }

interface DisplayRow {
  name: string
  title: string
  blurb: string
  aliasOf?: string
}

const display = (contractJson as unknown as {
  expToolDisplay: {
    prefix: string
    groups: { id: string; label: string; tools: string[] }[]
    tools: DisplayRow[]
  }
}).expToolDisplay

/** The group id of a display row: the tool it belongs to (the verb by its
 *  own name, a noun's op by `<noun>_`, the longest tool name wins so
 *  `sessions_send` stays Work); null for a retired alias row. Mirrors the
 *  generator's `expToolRowGroups` ×3. */
export function expToolRowGroup(row: { name: string; aliasOf?: string }): string | null {
  if (row.aliasOf) return null
  let best: { id: string; length: number } | null = null
  for (const group of display.groups) {
    for (const tool of group.tools) {
      if (row.name !== tool && !row.name.startsWith(`${tool}_`)) continue
      if (!best || tool.length > best.length) best = { id: group.id, length: tool.length }
    }
  }
  return best?.id ?? null
}

export interface BuiltinTool {
  /** The wire name (`exponential_issues_create`): the row's tooltip. */
  name: string
  title: string
  blurb: string
}

export interface BuiltinToolBand {
  id: string
  label: string
  tools: BuiltinTool[]
}

/** The filter: a case-insensitive substring over the title, the blurb and
 *  the wire name; blank = every tool. */
export function builtinToolMatches(tool: BuiltinTool, query: string): boolean {
  const needle = query.trim().toLowerCase()
  if (!needle) return true
  return (
    tool.title.toLowerCase().includes(needle) ||
    tool.blurb.toLowerCase().includes(needle) ||
    tool.name.toLowerCase().includes(needle)
  )
}

/** The bands the BUILT-IN section shows for a filter query, in group order;
 *  a band with no match is left out, so every band filtered away = [] (the
 *  page shows the `builtin-tools-no-match` empty state). */
export function builtinToolBands(query: string): BuiltinToolBand[] {
  return display.groups
    .map((group) => ({
      id: group.id,
      label: group.label,
      tools: display.tools
        .filter((row) => expToolRowGroup(row) === group.id)
        .map((row) => ({ name: `${display.prefix}${row.name}`, title: row.title, blurb: row.blurb }))
        .filter((tool) => builtinToolMatches(tool, query)),
    }))
    .filter((band) => band.tools.length > 0)
}

// Owner 2026-11-12: ONE Servers group, every row expandable to its tools.

export type McpBuiltinId = keyof typeof fixture.builtins
export type McpToolsState = `list` | `connect` | `stdio`

/** The Servers group's order: built-ins first (Exponential, then Team apps
 *  when an app exposes tools), then the team's servers in list order. A
 *  built-in is `builtin:<id>`, a team server its own key. */
export function mcpServerOrder<T>(appTools: boolean, servers: readonly T[]): (`builtin:${McpBuiltinId}` | T)[] {
  return [`builtin:exponential` as const, ...(appTools ? [`builtin:apps` as const] : []), ...servers]
}

/** What an open team server row shows under it, from the viewer's own
 *  connection: `list` (tRPC `mcpServers.tools`), `connect` (sign in first)
 *  or `stdio` (a local command lists its tools where a session runs it). */
export function mcpToolsState(server: { transport: string; status: string }): McpToolsState {
  if (server.transport !== `http`) return `stdio`
  return server.status === `connected` || server.status === `not_needed` ? `list` : `connect`
}

/** A built-in's URL: the instance origin + its path. */
export function builtinEndpoint(instance: string, id: McpBuiltinId): string {
  return `${instance.replace(/\/+$/, ``)}${fixture.builtins[id].path}`
}

/** "Could not list its tools: <reason>". */
export function toolsFailedLine(reason: string): string {
  return fixture.toolsFailed.replace(`{reason}`, reason)
}
