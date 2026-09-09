// EXP-792: a device's readiness report for the team MCP servers it can
// reach — shared by `mcpServers.reportReadiness` and the heartbeat's
// `mcpReadiness` field (devices.ts), so it lives outside both routers.
import { z } from "zod"
import { and, inArray } from "drizzle-orm"
import { mcpServers } from "@/db/schema"
import { getUserTeamIds } from "@/lib/team-membership"
import { expiresAtOrNull, upsertReadiness, type Db } from "@/lib/mcp-oauth/flows"

const readinessEntrySchema = z.object({
  serverId: z.string().uuid(),
  ready: z.boolean(),
  expiresAt: z.string().max(64).optional(),
  error: z.string().max(500).optional(),
})
export const mcpReadinessEntriesSchema = z.array(readinessEntrySchema).max(64)
export type McpReadinessEntry = z.infer<typeof readinessEntrySchema>

/** Upsert a device's readiness report for the servers of the caller's teams;
 * ids outside them (a stale local list, a foreign uuid) are dropped, never
 * an error — the device reports what it holds. Shared with
 * `devices.heartbeat` (`mcpReadiness`). */
export async function applyReadinessReport(
  db: Db,
  args: {
    userId: string
    deviceRowId: string
    entries: McpReadinessEntry[]
  },
  now = new Date()
): Promise<void> {
  if (args.entries.length === 0) return
  const teamIds = await getUserTeamIds(args.userId)
  if (teamIds.length === 0) return
  const ids = [...new Set(args.entries.map((entry) => entry.serverId))]
  const known = await db
    .select({ id: mcpServers.id })
    .from(mcpServers)
    .where(
      and(inArray(mcpServers.id, ids), inArray(mcpServers.teamId, teamIds))
    )
  const allowed = new Set(known.map((row) => row.id))
  // Last entry per server wins (a device that lists one twice meant the
  // later read).
  const latest = new Map<string, McpReadinessEntry>()
  for (const entry of args.entries) {
    if (allowed.has(entry.serverId)) latest.set(entry.serverId, entry)
  }
  for (const entry of latest.values()) {
    await upsertReadiness(
      db,
      {
        serverId: entry.serverId,
        deviceRowId: args.deviceRowId,
        userId: args.userId,
        ready: entry.ready,
        expiresAt: expiresAtOrNull(entry.expiresAt),
        error: entry.error ?? null,
      },
      now
    )
  }
}
