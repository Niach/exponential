import { eq, inArray } from "drizzle-orm"
import { launchPick, mergeTeamPicks, teamPick } from "@exp/domain-contract/agent-settings"
import { mcpServers, skills, teams } from "@/db/schema"
import type { db as Db } from "@/db/connection"
import { sessionSkillIds } from "@/lib/skills/builtin"

// #EXP-1988 (agent-settings.json `assistant.teamPicks`): the Assistant's
// skills and MCP servers PER TEAM. Every `skills` / `mcp_servers` row belongs
// to exactly one team, so the flat `agents.skill_ids` / `mcp_server_ids`
// (and a session's copy) ARE the pick per team: the picks for T = the ids
// whose row is T's. Writes merge per team (`mergeTeamPicks`); a launch takes
// the rows of its launch teams (`launchPick`).

type Reader = Pick<typeof Db, `select`>
export type PickKind = `skills` | `mcp`

/** Row id → its team, for the rows that still exist. */
export async function rowTeamsOf(reader: Reader, kind: PickKind, ids: readonly string[]): Promise<Record<string, string>> {
  const unique = [...new Set(ids)]
  if (unique.length === 0) return {}
  const table = kind === `skills` ? skills : mcpServers
  const rows = await reader
    .select({ id: table.id, teamId: table.teamId })
    .from(table)
    .where(inArray(table.id, unique))
  return Object.fromEntries(rows.map((row) => [row.id, row.teamId]))
}

/** An agent's list after a write (`mergeCases`). */
export async function mergedAgentPick(
  reader: Reader,
  kind: PickKind,
  input: { existing: readonly string[] | null; written: readonly string[]; pickTeamId: string | null; agentTeamId: string }
): Promise<string[]> {
  const existing = input.existing ?? []
  return mergeTeamPicks({
    existing,
    written: input.written,
    rowTeams: await rowTeamsOf(reader, kind, [...existing, ...input.written]),
    pickTeamId: input.pickTeamId,
    agentTeamId: input.agentTeamId,
  })
}

/** The picks of `teamId` in a list (null = none). */
export async function pickOfTeam(
  reader: Reader,
  kind: PickKind,
  ids: readonly string[] | null | undefined,
  teamId: string
): Promise<string[] | null> {
  if (!ids || ids.length === 0) return null
  const picked = teamPick(ids, await rowTeamsOf(reader, kind, ids), teamId)
  return picked.length > 0 ? picked : null
}

async function isPersonalTeam(reader: Reader, teamId: string): Promise<boolean> {
  const [row] = await reader.select({ kind: teams.kind }).from(teams).where(eq(teams.id, teamId)).limit(1)
  return row?.kind === `personal`
}

/** A session's LAUNCH TEAMS (`launchCases`): a session in a regular team =
 *  its team; a session of the Assistant's tree in the personal team = the
 *  tree's teams (`treeTeams` = the gate's `assistantScope`). */
export async function sessionLaunchTeams(
  reader: Reader,
  session: { teamId: string; rootId: string | null }
): Promise<string[]> {
  if (!(await isPersonalTeam(reader, session.teamId))) return [session.teamId]
  const { treeTeams } = await import(`@/lib/sessions/tree-teams`)
  const teamIds = session.rootId ? await treeTeams(session.rootId, reader) : []
  return teamIds.length > 0 ? teamIds : [session.teamId]
}

/** What a session launches with: its list ∩ its launch teams' rows (a gone
 *  row stays, skipped with a reason at launch). */
export async function sessionLaunchPick(
  reader: Reader,
  kind: PickKind,
  session: { teamId: string; rootId: string | null },
  ids: readonly string[] | null | undefined
): Promise<string[]> {
  if (!ids || ids.length === 0) return []
  return launchPick(ids, await rowTeamsOf(reader, kind, ids), await sessionLaunchTeams(reader, session))
}

/** A session's effective skill pick: its launch pick, then (an app editor
 *  session) the built-ins it always runs with (`sessionSkillIds`). */
export async function sessionLaunchSkillIds(
  reader: Reader,
  session: { teamId: string; rootId: string | null; skillIds: string[] | null; appId: string | null }
): Promise<string[]> {
  const skillIds = await sessionLaunchPick(reader, `skills`, session, session.skillIds)
  return sessionSkillIds(reader as never, { teamId: session.teamId, skillIds, appId: session.appId })
}

/** Launch entries of different teams' servers with the same name get `-2`,
 *  `-3`… in launch order. */
export function uniqueLaunchNames<T extends { name: string }>(entries: T[]): T[] {
  const taken = new Set<string>()
  return entries.map((entry) => {
    let name = entry.name
    for (let n = 2; taken.has(name.toLowerCase()); n++) name = `${entry.name}-${n}`
    taken.add(name.toLowerCase())
    return name === entry.name ? entry : { ...entry, name }
  })
}
