import { and, eq, inArray, isNull, sql } from "drizzle-orm"
import type { Issue } from "@/db/schema"
import { attachments, issueLabels, issues } from "@/db/schema"
import {
  addRecurrence,
  formatDateForMutation,
  getIssueDescriptionText,
} from "@/lib/domain"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  extractMarkdownImageOccurrences,
  getAttachmentIdFromUrl,
  removeMarkdownImagesByUrl,
  replaceMarkdownImageUrls,
} from "@/lib/storage/issue-attachments"
import { copyObject } from "@/lib/storage"

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

interface CloneIssueForRecurrenceParams {
  sourceIssueId: string
  sourceProjectId: string
  sourceWorkspaceId: string
  sourceTitle: string
  sourcePriority: Issue[`priority`]
  sourceAssigneeId: Issue[`assigneeId`]
  sourceDescription: Issue[`description`]
  recurrenceInterval: number
  recurrenceUnit: NonNullable<Issue[`recurrenceUnit`]>
  creatorId: string
  /** Used to resolve image URLs in the source description to attachment ids. */
  requestUrl: string
}

/** A storage object that must be duplicated after the clone transaction commits. */
export interface AttachmentCopyOp {
  sourceKey: string
  destKey: string
}

export interface CloneIssueForRecurrenceResult {
  issue: Issue
  attachmentCopies: AttachmentCopyOp[]
}

export async function cloneIssueForRecurrence(
  tx: Tx,
  params: CloneIssueForRecurrenceParams
): Promise<CloneIssueForRecurrenceResult> {
  const nextDueDate = formatDateForMutation(
    addRecurrence(new Date(), params.recurrenceInterval, params.recurrenceUnit)
  )

  const sourceText = getIssueDescriptionText(params.sourceDescription)

  // Pre-generate the clone's id so attachment storage keys (which nest under
  // `issues/{issueId}/`) and the rewritten image URLs can reference it before
  // the row exists.
  const cloneIssueId = crypto.randomUUID()

  // Each recurring instance owns its own attachments — the attachment lifecycle
  // is strictly per-issue (cascade delete, per-issue orphan reclaim, and the
  // update mutation rejects descriptions that reference another issue's
  // attachments). So we deep-clone every image the source references: new rows
  // owned by the clone, fresh storage objects, and rewritten markdown URLs.
  const occurrences = extractMarkdownImageOccurrences(sourceText)
  const referencedIds = new Set<string>()
  for (const occurrence of occurrences) {
    const attachmentId = getAttachmentIdFromUrl(occurrence.url, params.requestUrl)
    if (attachmentId) referencedIds.add(attachmentId)
  }

  const attachmentCopies: AttachmentCopyOp[] = []
  let clonedRows: Array<typeof attachments.$inferInsert> = []
  let clonedText = sourceText

  if (referencedIds.size > 0) {
    const sourceAttachments = await tx
      .select({
        id: attachments.id,
        uploaderId: attachments.uploaderId,
        filename: attachments.filename,
        contentType: attachments.contentType,
        sizeBytes: attachments.sizeBytes,
        storageKey: attachments.storageKey,
        width: attachments.width,
        height: attachments.height,
      })
      .from(attachments)
      .where(
        and(
          eq(attachments.issueId, params.sourceIssueId),
          // Only the issue's own (non-comment) attachments — a comment-scoped
          // attachment has its own lifecycle and must not be cloned onto the
          // next occurrence.
          isNull(attachments.commentId),
          inArray(attachments.id, [...referencedIds])
        )
      )

    const idToNewId = new Map<string, string>()
    clonedRows = sourceAttachments.map((row) => {
      const newId = crypto.randomUUID()
      const newKey = buildAttachmentStorageKey(cloneIssueId, newId, row.filename)
      idToNewId.set(row.id, newId)
      attachmentCopies.push({ sourceKey: row.storageKey, destKey: newKey })
      return {
        id: newId,
        workspaceId: params.sourceWorkspaceId,
        issueId: cloneIssueId,
        uploaderId: row.uploaderId,
        filename: row.filename,
        contentType: row.contentType,
        sizeBytes: row.sizeBytes,
        storageKey: newKey,
        url: buildAttachmentUrl(newId),
        width: row.width,
        height: row.height,
      }
    })

    // Rewrite each cloned image's URL to its clone's relative URL (keyed by the
    // exact url found, so non-canonical legacy urls are handled too), and strip
    // any referenced attachment we could not clone — a missing row, or one we
    // intentionally skipped (comment-scoped). This keeps the clone's
    // description referencing only its own attachments and never leaves a
    // dangling source-issue reference that would later fail the
    // "attachment belongs to this issue" guard on edit.
    const replacements = new Map<string, string>()
    const removableUrls: string[] = []
    for (const occurrence of occurrences) {
      const attachmentId = getAttachmentIdFromUrl(
        occurrence.url,
        params.requestUrl
      )
      if (!attachmentId) continue
      const newId = idToNewId.get(attachmentId)
      if (newId) {
        replacements.set(occurrence.url, buildAttachmentUrl(newId))
      } else {
        removableUrls.push(occurrence.url)
      }
    }
    clonedText = replaceMarkdownImageUrls(sourceText, replacements)
    if (removableUrls.length > 0) {
      clonedText = removeMarkdownImagesByUrl(clonedText, removableUrls)
    }
  }

  // Insert the issue before its attachments so the attachments' issue_id FK is
  // satisfied (the clone id is set explicitly here, not by the column default).
  const [insertedClone] = await tx
    .insert(issues)
    .values({
      id: cloneIssueId,
      projectId: params.sourceProjectId,
      title: params.sourceTitle,
      priority: params.sourcePriority,
      assigneeId: params.sourceAssigneeId,
      description: clonedText ? clonedText : null,
      status: `todo`,
      dueDate: nextDueDate,
      recurrenceInterval: params.recurrenceInterval,
      recurrenceUnit: params.recurrenceUnit,
      creatorId: params.creatorId,
    })
    .returning()

  if (clonedRows.length > 0) {
    await tx.insert(attachments).values(clonedRows)
  }

  await tx.execute(sql`
    INSERT INTO ${issueLabels} (issue_id, label_id, workspace_id)
    SELECT ${cloneIssueId}::uuid, label_id, workspace_id
    FROM ${issueLabels}
    WHERE issue_id = ${params.sourceIssueId}::uuid
  `)

  return { issue: insertedClone, attachmentCopies }
}

/**
 * Duplicates the source attachments' storage objects into their clones' keys.
 * Best-effort and run after the clone transaction commits (so we never hold a
 * DB transaction open across S3 round-trips); a failed copy leaves the clone's
 * image URL resolving to a missing object, which is logged rather than thrown.
 */
export async function copyRecurrenceAttachments(
  copies: AttachmentCopyOp[]
): Promise<void> {
  if (copies.length === 0) return
  await Promise.allSettled(
    copies.map(async ({ sourceKey, destKey }) => {
      try {
        await copyObject({ sourceKey, destKey })
      } catch (error) {
        console.error(
          `Failed to copy recurring-issue attachment object ${sourceKey} -> ${destKey}`,
          error
        )
      }
    })
  )
}
