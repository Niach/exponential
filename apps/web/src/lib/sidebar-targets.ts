import { and, eq, inArray, isNull, or } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import type { db } from "@/db/connection"
import { agents, apps, boards, sessions } from "@/db/schema"

type Exec = Pick<typeof db, `select`>

// #EXP-2027 (sidebar-tree.json): an item of a team's `team` list places one
// of THAT team's boards, agents (never an Assistant), apps or root sessions
// the caller sees (starter or shared). The kind is never stored: the id is
// looked up in each table.
export async function assertSidebarTargetsInTeam(
  exec: Exec,
  userId: string,
  teamId: string,
  targetIds: readonly string[]
): Promise<void> {
  const ids = [...new Set(targetIds)]
  if (ids.length === 0) return
  const [boardRows, agentRows, appRows, sessionRows] = await Promise.all([
    exec.select({ id: boards.id }).from(boards).where(and(eq(boards.teamId, teamId), inArray(boards.id, ids))),
    exec
      .select({ id: agents.id })
      .from(agents)
      .where(and(eq(agents.teamId, teamId), eq(agents.role, `agent`), inArray(agents.id, ids))),
    exec.select({ id: apps.id }).from(apps).where(and(eq(apps.teamId, teamId), inArray(apps.id, ids))),
    exec
      .select({ id: sessions.id })
      .from(sessions)
      .where(
        and(
          eq(sessions.teamId, teamId),
          isNull(sessions.parentId),
          or(eq(sessions.userId, userId), eq(sessions.shared, true)),
          inArray(sessions.id, ids)
        )
      ),
  ])
  const found = new Set([...boardRows, ...agentRows, ...appRows, ...sessionRows].map((row) => row.id))
  if (ids.some((id) => !found.has(id))) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Not a board, agent, app or session of this team` })
  }
}
