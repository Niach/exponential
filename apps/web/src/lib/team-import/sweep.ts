// #EXP-1713: an import nobody committed (or whose commit never succeeded)
// is purged with its team after fixtures/team-import.json
// `abandonedAfterHours`: the staged rows and the id map cascade with the
// import row, the team cascades its landed attachments' rows, and their
// objects are deleted first. Runs from the import worker's sweep.
import { and, eq, inArray, lt } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, teamImports, teams } from "@/db/schema"
import { deleteObject } from "@/lib/storage"
import { TEAM_IMPORT_ABANDONED_AFTER_HOURS } from "@exp/domain-contract/team-import"

export const TEAM_IMPORT_ABANDONED_MS = TEAM_IMPORT_ABANDONED_AFTER_HOURS * 60 * 60 * 1000

export async function sweepAbandonedTeamImports(now: Date = new Date()): Promise<number> {
  const stale = await db
    .select({ id: teamImports.id, teamId: teamImports.teamId })
    .from(teamImports)
    .where(and(inArray(teamImports.status, [`importing`, `failed`]), lt(teamImports.createdAt, new Date(now.getTime() - TEAM_IMPORT_ABANDONED_MS))))
  for (const imp of stale) {
    const objects = await db.select({ storageKey: attachments.storageKey }).from(attachments).where(eq(attachments.teamId, imp.teamId))
    for (const object of objects) {
      try {
        await deleteObject(object.storageKey)
      } catch (error) {
        console.error(`[team-import] could not delete ${object.storageKey}`, error)
      }
    }
    // The team cascades the import row and every landed attachment.
    await db.delete(teams).where(eq(teams.id, imp.teamId))
  }
  return stale.length
}
