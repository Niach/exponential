// EXP-792: every picked MCP server must be a row of the subject's team —
// shared by steer's remote starts and (FEED-73) an action's own list
// (`actions.create/update({mcpServerIds})`). Duplicates collapse (first
// order wins); the count check refuses a foreign or vanished id without
// naming which (the caller's own picker rendered the list). Nothing picked
// → undefined, no query.
import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import { mcpServers } from "@/db/schema"

export async function assertMcpServersInTeam(
  mcpServerIds: string[] | undefined,
  teamId: string
): Promise<string[] | undefined> {
  if (!mcpServerIds || mcpServerIds.length === 0) return undefined
  const ids = [...new Set(mcpServerIds)]
  const { db } = await import(`@/db/connection`)
  const rows = await db
    .select({ id: mcpServers.id })
    .from(mcpServers)
    .where(and(inArray(mcpServers.id, ids), eq(mcpServers.teamId, teamId)))
    .limit(ids.length)
  const found = new Set(rows.map((row) => row.id))
  if (ids.some((id) => !found.has(id))) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `One of the picked MCP servers is not in this team (removed?)`,
    })
  }
  return ids
}
