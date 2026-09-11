import { inArray } from "drizzle-orm"
import { attachments, sessionAttachments } from "@/db/schema"
import type { db as Database } from "@/db/connection"
import { collectAttachmentStorageKeys } from "@/lib/storage/issue-attachments"

// Works over the root db or a transaction — structurally typed so callers
// can collect inside their own delete transaction.
type SelectCapable = Pick<typeof Database, `select`>

/**
 * Every S3 object a team-cascade delete would strand: issue attachments plus
 * the server-only steer images (EXP-702). Collect INSIDE the deleting
 * transaction BEFORE the cascade drops the rows — the
 * cascade never touches S3. Deduped (widget screenshots can't collide, but
 * cheap insurance against future sharing).
 */
export async function collectTeamStorageKeys(
  tx: SelectCapable,
  teamIds: string[]
): Promise<string[]> {
  if (teamIds.length === 0) return []
  const [issueRows, sessionRows] = await Promise.all([
    tx
      .select({
        storageKey: attachments.storageKey,
        posterStorageKey: attachments.posterStorageKey,
      })
      .from(attachments)
      .where(inArray(attachments.teamId, teamIds)),
    tx
      .select({ storageKey: sessionAttachments.storageKey })
      .from(sessionAttachments)
      .where(inArray(sessionAttachments.teamId, teamIds)),
  ])
  // EXP-824: video rows own a poster blob too — collected by the helper.
  return collectAttachmentStorageKeys([...issueRows, ...sessionRows])
}
