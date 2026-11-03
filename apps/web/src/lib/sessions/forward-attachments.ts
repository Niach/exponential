import { and, desc, eq, inArray, isNotNull, isNull, or } from "drizzle-orm"
import { attachments, messages } from "@/db/schema"
import { copyObject, deleteObject } from "@/lib/storage"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  buildPendingSessionAttachmentStorageKey,
} from "@/lib/storage/issue-attachments"
import { assertWithinStorageLimit } from "@/lib/billing"
import { inSession, pendingInput } from "@/lib/sessions/messages"
import type { Db, Tx } from "@/lib/sessions/tx"

// #EXP-1682: FORWARDING a file a sender can already read (a person's pasted
// screenshot on the message an agent is answering) into another message or
// an issue comment, with no re-upload. The file gets a NEW row and its own
// blob (a server-side copy), so every reclaim path keeps keying on one row
// per object. Sealing (#EXP-1458) seals bodies and cards, never files: the
// bytes, filename and `messages.attachment_ids` of a sealed message are
// already in clear on the server, so a copy exposes nothing new; a sealed
// target body is never touched.

/** A message's file the readers may see: same team, owned by a message
 *  (never an issue, draft or app record) that is shared or theirs. */
async function readableMessageFiles(
  tx: Pick<Db, `select`>,
  args: { ids: string[]; teamId: string; readerIds: string[] }
) {
  if (args.ids.length === 0) return []
  return tx
    .select({
      id: attachments.id,
      filename: attachments.filename,
      contentType: attachments.contentType,
      sizeBytes: attachments.sizeBytes,
      storageKey: attachments.storageKey,
      width: attachments.width,
      height: attachments.height,
      durationMs: attachments.durationMs,
      posterStorageKey: attachments.posterStorageKey,
      posterSizeBytes: attachments.posterSizeBytes,
      asFile: attachments.asFile,
    })
    .from(attachments)
    .innerJoin(messages, eq(messages.id, attachments.messageId))
    .where(
      and(
        inArray(attachments.id, args.ids),
        eq(attachments.teamId, args.teamId),
        isNull(attachments.issueId),
        isNull(attachments.draftId),
        isNull(attachments.appId),
        or(eq(messages.shared, true), inArray(messages.ownerId, args.readerIds))
      )
    )
}

export type ForwardOwner = { kind: `message`; messageId: string } | { kind: `issue`; issueId: string }

/**
 * Copies each readable message file in `ids` onto `owner` (uploaded by
 * `uploaderId`): `Map(source id → new id)`. Ids that are not readable
 * message files are simply absent (the caller's own rules judge them).
 * A blob copied before a later failure is deleted again.
 */
export async function forwardMessageFiles(
  tx: Tx,
  args: { ids: string[]; teamId: string; readerIds: string[]; uploaderId: string; owner: ForwardOwner }
): Promise<Map<string, string>> {
  const rows = await readableMessageFiles(tx, args)
  const forwarded = new Map<string, string>()
  if (rows.length === 0) return forwarded
  const bytes = rows.reduce((sum, row) => sum + row.sizeBytes + (row.posterSizeBytes ?? 0), 0)
  await assertWithinStorageLimit(args.teamId, bytes)
  const copied: string[] = []
  try {
    for (const row of rows) {
      const id = crypto.randomUUID()
      const storageKey =
        args.owner.kind === `issue`
          ? buildAttachmentStorageKey(args.owner.issueId, id, row.filename)
          : buildPendingSessionAttachmentStorageKey(args.teamId, id, row.filename)
      await copyObject(row.storageKey, storageKey)
      copied.push(storageKey)
      let posterStorageKey: string | null = null
      if (row.posterStorageKey) {
        posterStorageKey = `${storageKey}.poster`
        await copyObject(row.posterStorageKey, posterStorageKey)
        copied.push(posterStorageKey)
      }
      await tx.insert(attachments).values({
        id,
        teamId: args.teamId,
        uploaderId: args.uploaderId,
        ...(args.owner.kind === `issue` ? { issueId: args.owner.issueId } : { messageId: args.owner.messageId }),
        filename: row.filename,
        contentType: row.contentType,
        sizeBytes: row.sizeBytes,
        storageKey,
        url: buildAttachmentUrl(id),
        width: row.width,
        height: row.height,
        durationMs: row.durationMs,
        posterStorageKey,
        posterSizeBytes: row.posterSizeBytes,
        asFile: row.asFile,
      })
      forwarded.set(row.id, id)
    }
  } catch (error) {
    await Promise.all(copied.map((key) => deleteObject(key).catch(() => {})))
    throw error
  }
  return forwarded
}

/** `forwardAttachments: true`: the files of the newest message delivered
 *  INTO `sessionId` (a person's, or another session's) that carries any:
 *  the message the session is answering. */
export async function answeredMessageFiles(tx: Pick<Db, `select`>, sessionId: string): Promise<string[]> {
  const [row] = await tx
    .select({ attachmentIds: messages.attachmentIds })
    .from(messages)
    .where(and(inSession(sessionId), pendingInput(), isNotNull(messages.attachmentIds)))
    .orderBy(desc(messages.createdAt))
    .limit(1)
  return row?.attachmentIds ?? []
}

/** A plaintext body naming a forwarded file by its SOURCE url names the
 *  copy instead (the viewers and the device resolve the copy). */
export function retargetBody(body: string, forwarded: Map<string, string>): string {
  let next = body
  for (const [from, to] of forwarded) next = next.split(buildAttachmentUrl(from)).join(buildAttachmentUrl(to))
  return next
}
