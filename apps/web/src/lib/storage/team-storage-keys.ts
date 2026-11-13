import { inArray } from "drizzle-orm"
import { attachments } from "@/db/schema"
import type { db as Database } from "@/db/connection"
import { collectAttachmentStorageKeys } from "@/lib/storage/issue-attachments"

// Works over the root db or a transaction — structurally typed so callers
// can collect inside their own delete transaction.
type SelectCapable = Pick<typeof Database, `select`>

/**
 * Every S3 object a team-cascade delete would strand: every `attachments`
 * row of the team (issue and draft files, and since EXP-1291 conversation
 * files and Guide pictures). Collect INSIDE the deleting transaction BEFORE
 * the cascade drops the rows — the cascade never touches S3. Deduped.
 */
export async function collectTeamStorageKeys(
  tx: SelectCapable,
  teamIds: string[]
): Promise<string[]> {
  if (teamIds.length === 0) return []
  const rows = await tx
    .select({
      storageKey: attachments.storageKey,
      posterStorageKey: attachments.posterStorageKey,
    })
    .from(attachments)
    .where(inArray(attachments.teamId, teamIds))
  // EXP-824: video rows own a poster blob too — collected by the helper.
  return collectAttachmentStorageKeys(rows)
}
