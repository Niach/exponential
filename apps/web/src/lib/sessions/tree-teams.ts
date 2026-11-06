import { eq } from "drizzle-orm"
import { assistantScope } from "@exp/emcp/access"
import { db } from "@/db/connection"
import { agents, sessions } from "@/db/schema"
import { getMemberTeamIds } from "@/lib/auth/membership"

// #EXP-1720 (eagent tree.json `treeTeams`, emcp access.json
// `assistantScope`): the teams a session tree works in. Every tree = its
// root's own team, except the Assistant's conversation: its own (personal)
// team, then the member's Teams pick ∩ the teams they belong to NOW (NULL =
// all). Its children and threads start in one of these; the gate clamps an
// acting-agent request to them; a watcher watches only these.

type Exec = Pick<typeof db, `select`>

export interface TreeRoot {
  teamId: string
  userId: string
  agentId: string | null
  scopeTeamIds: string[] | null
}

/** The teams of the tree rooted at `root`. */
export async function treeTeamsOfRoot(root: TreeRoot, exec: Exec = db): Promise<string[]> {
  if (!root.agentId) return [root.teamId]
  const [agent] = await exec.select({ role: agents.role }).from(agents).where(eq(agents.id, root.agentId)).limit(1)
  if (agent?.role !== `assistant`) return [root.teamId]
  return assistantScope({
    personalTeamId: root.teamId,
    memberTeamIds: await getMemberTeamIds(root.userId),
    pickedTeamIds: root.scopeTeamIds,
  })
}

/** The teams of the tree `rootId` (its root gone = none). */
export async function treeTeams(rootId: string, exec: Exec = db): Promise<string[]> {
  const [root] = await exec
    .select({
      teamId: sessions.teamId,
      userId: sessions.userId,
      agentId: sessions.agentId,
      scopeTeamIds: sessions.scopeTeamIds,
    })
    .from(sessions)
    .where(eq(sessions.id, rootId))
    .limit(1)
  return root ? treeTeamsOfRoot(root, exec) : []
}
