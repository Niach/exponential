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
