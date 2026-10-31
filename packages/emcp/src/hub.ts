// EXP-1507: eMCP's LOCAL HUB, several MCP servers behind one loopback
// endpoint with a grant per run and an `mcp_calls` row per call. Pure and
// locked by `fixtures/hub.json` (its `$comment` is the spec); the `emcp`
// crate's `hub` replays the same cases.

import fixture from "../fixtures/hub.json" with { type: "json" }
import { type Level, toolVisible } from "./access"
import { argsHash, type CallStatus } from "./audit"
import { listToolNames, splitToolName, type UpstreamAnnotations, upstreamAccess } from "./exposes"

export type HubOutcome = keyof typeof fixture.outcomes

/** What a run may reach: a level per mounted server (not named = none). */
export interface HubGrant {
  servers: Record<string, Exclude<Level, `none`>>
  teamId: string | null
  userId: string | null
}

export interface HubTool {
  name: string
  annotations?: UpstreamAnnotations | null
}

export interface HubServer {
  name: string
  tools: readonly HubTool[]
}

export interface HubRoute {
  server: string
  tool: string
}

const SERVER_NAME = new RegExp(fixture.serverNamePattern)

/** May a server be mounted under `name`? */
export function hubServerNameValid(name: string): boolean {
  return name.length <= fixture.serverNameMax && SERVER_NAME.test(name)
}

/** The run's tools/list: every tool its grant lets it see, qualified. */
export function hubToolList(grant: HubGrant, servers: readonly HubServer[]): { app: string; tool: string; name: string }[] {
  const visible = servers.map((server) => {
    const level: Level = grant.servers[server.name] ?? `none`
    return {
      slug: server.name,
      exposes: server.tools.filter(
        (tool) => !tool.name.includes(`__`) && toolVisible(level, upstreamAccess(tool.annotations)),
      ),
    }
  })
  return listToolNames(visible).filter((listed) => splitToolName(listed.name) !== null)
}

/** Where a call goes: a name on the run's own list, else null (unknown). */
export function hubRoute(grant: HubGrant, servers: readonly HubServer[], name: string): HubRoute | null {
  const listed = hubToolList(grant, servers).find((tool) => tool.name === name)
  return listed ? { server: listed.app, tool: listed.tool } : null
}

/** The row's status and error code for an outcome. */
export function hubOutcomeStatus(outcome: HubOutcome): { status: CallStatus; error: string | null } {
  return { status: fixture.outcomes[outcome] as CallStatus, error: fixture.errors[outcome] }
}

export interface HubAuditRow {
  id: string
  teamId: string | null
  userId: string | null
  clientId: string
  appId: null
  tool: string
  status: CallStatus
  error: string | null
  latencyMs: number
  argsHash: string
  createdAt: string
}

/** The `mcp_calls` row of one hub call (`outcome` = `unknown` when it was
 *  not routed). */
export function hubAuditRow(input: {
  id: string
  grant: HubGrant
  run: string
  name: string
  args: unknown
  outcome: HubOutcome
  latencyMs: number
  createdAtMs: number
}): HubAuditRow {
  const { status, error } = hubOutcomeStatus(input.outcome)
  return {
    id: input.id,
    teamId: input.grant.teamId,
    userId: input.grant.userId,
    clientId: input.run,
    appId: null,
    tool: input.name,
    status,
    error,
    latencyMs: Math.max(0, Math.floor(input.latencyMs)),
    argsHash: argsHash(input.args),
    createdAt: new Date(input.createdAtMs).toISOString(),
  }
}
