import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import type { db as Db } from "@/db/connection"
import { skills } from "@/db/schema"

// #EXP-1656: a session's (or an agent's) skill pick — deduped, pick order,
// refusing any id outside the team. An empty pick = none (null), exactly
// like the MCP servers' pick (lib/sessions/send.ts assertMcpServersInTeam).
type Reader = Pick<typeof Db, `select`>

export async function assertSkillsInTeam(
  reader: Reader,
  teamId: string,
  ids: readonly string[] | undefined
): Promise<string[] | null> {
  const unique = [...new Set(ids ?? [])]
  if (unique.length === 0) return null
  const rows = await reader
    .select({ id: skills.id })
    .from(skills)
    .where(and(eq(skills.teamId, teamId), inArray(skills.id, unique)))
  if (rows.length !== unique.length) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Unknown skill for this team` })
  }
  return unique
}
