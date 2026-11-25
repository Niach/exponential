import { and, eq } from "drizzle-orm"
import { treeScope, type TreeRootKind } from "@exp/emcp/access"
import { db } from "@/db/connection"
import { agents, sessions, teams } from "@/db/schema"
import { getMemberTeamIds } from "@/lib/auth/membership"

// #EXP-1720 + #EXP-2027 (eagent tree.json `treeTeams`, emcp access.json
// `treeScope`): the teams a session tree works in. A team agent's tree =
// its root's own team; the Assistant's conversation = its own (personal)
// team, then the member's Teams pick ∩ the teams they belong to NOW (NULL
// = all); a plain chat (no acting agent, or one the Assistant's tree
// started) = its own team, then its Teams pick ∩ their teams (NULL = none
// picked: its own team only). Its children and chats start in one of
// these; the gate clamps every request speaking for one of its sessions to
// them; a watcher watches only these.

type Exec = Pick<typeof db, `select`>

export interface TreeRoot {
  teamId: string
  userId: string
  agentId: string | null
  /** The agent whose tree started this root (`started_by_agent_id`). */
  startedByAgentId?: string | null
  scopeTeamIds: string[] | null
}

/** #EXP-2027: what `root` is: the Assistant's conversation, a team agent's
 *  tree (its conversation or a root it started) or a plain chat (no acting
 *  agent, or the Assistant's). */
export async function treeRootKind(
  root: Pick<TreeRoot, `agentId` | `startedByAgentId`>,
  exec: Exec = db
): Promise<TreeRootKind> {
  const acting = root.agentId ?? root.startedByAgentId ?? null
  if (!acting) return `chat`
  const [agent] = await exec.select({ role: agents.role }).from(agents).where(eq(agents.id, acting)).limit(1)
  if (agent?.role !== `assistant`) return `agent`
  return root.agentId ? `assistant` : `chat`
}

/** What the tree rooted at `root` is and the teams it works in. */
export async function treeReachOfRoot(root: TreeRoot, exec: Exec = db): Promise<{ kind: TreeRootKind; teams: string[] }> {
  const kind = await treeRootKind(root, exec)
  if (kind === `agent` || (kind === `chat` && !root.scopeTeamIds?.length)) return { kind, teams: [root.teamId] }
  const teams = treeScope({
    root: kind,
    ownTeamId: root.teamId,
    memberTeamIds: await getMemberTeamIds(root.userId),
    pickedTeamIds: root.scopeTeamIds,
  })
  return { kind, teams }
}

/** The teams of the tree rooted at `root`. */
export async function treeTeamsOfRoot(root: TreeRoot, exec: Exec = db): Promise<string[]> {
  return (await treeReachOfRoot(root, exec)).teams
}

/** The teams of the tree `rootId` (its root gone = none). */
export async function treeTeams(rootId: string, exec: Exec = db): Promise<string[]> {
  return (await treeReach(rootId, exec))?.teams ?? []
}

/** `treeReachOfRoot` of the tree `rootId` (its root gone = null). */
export async function treeReach(rootId: string, exec: Exec = db): Promise<{ kind: TreeRootKind; teams: string[] } | null> {
  const [root] = await exec
    .select({
      teamId: sessions.teamId,
      userId: sessions.userId,
      agentId: sessions.agentId,
      startedByAgentId: sessions.startedByAgentId,
      scopeTeamIds: sessions.scopeTeamIds,
    })
    .from(sessions)
    .where(eq(sessions.id, rootId))
    .limit(1)
  return root ? treeReachOfRoot(root, exec) : null
}

/** #EXP-2107 (emcp access.json `treeScope`, `ownAssistant`): a plain chat's
 *  session may still ADDRESS its member's own Assistant conversation (message
 *  or answer it, stop it, read its tree, watch it) though that sits in the
 *  member's personal team, outside the chat's teams: the chats the Assistant
 *  started report back to it. Only when the request is the chat's own
 *  member's (never a shared device's host) and the tree is a plain chat
 *  (never a team agent's). The personal team's id, else null. */
export async function ownAssistantTeam(
  caller: { rootId: string; userId: string },
  requesterId: string,
  exec: Exec = db
): Promise<string | null> {
  if (caller.userId !== requesterId) return null
  if ((await treeReach(caller.rootId, exec))?.kind !== `chat`) return null
  const [row] = await exec
    .select({ id: teams.id })
    .from(teams)
    .where(and(eq(teams.ownerId, caller.userId), eq(teams.kind, `personal`)))
    .limit(1)
  return row?.id ?? null
}
