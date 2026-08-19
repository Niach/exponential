import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { and, desc, eq, ilike, inArray, isNotNull } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, comments, issues } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import {
  collectReferencedAttachmentIds,
  isAcceptedImageContentType,
} from "@/lib/storage/issue-attachments"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { replaceAttachmentReferencesInTx } from "@/lib/storage/attachment-references"

// Grace window for the owner-triggered sweep: an image uploaded moments ago
// may still be sitting in an unsaved editor draft, so never reclaim anything
// younger than this even when no markdown references it yet.
export const SWEEP_GRACE_MS = 24 * 60 * 60 * 1000

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

// Read-only surface shared by the connection and a transaction handle.
type Queryable = Pick<Tx, `select`>

/**
 * Every attachment id referenced by any issue description or comment body in
 * the team, in ONE pass: a cheap LIKE prefilter narrows the rows to texts that
 * mention the attachment route at all, then the exact markdown parser decides.
 */
async function collectTeamReferencedAttachmentIdsInTx(
  tx: Queryable,
  teamId: string,
  origin: string
): Promise<Set<string>> {
  const pattern = `%/api/attachments/%`

  const [descriptionRows, commentRows, commentLinkedRows] = await Promise.all([
    tx
      .select({ text: issues.description })
      .from(issues)
      .where(
        and(eq(issues.teamId, teamId), ilike(issues.description, pattern))
      ),
    tx
      .select({ text: comments.body })
      .from(comments)
      .where(and(eq(comments.teamId, teamId), ilike(comments.body, pattern))),
    // Comment attachments (EXP-554) are linked via comment_id, never embedded
    // in markdown — without this they'd read as unreferenced sweep bait.
    tx
      .select({ id: attachments.id })
      .from(attachments)
      .where(
        and(eq(attachments.teamId, teamId), isNotNull(attachments.commentId))
      ),
  ])

  const referenced = collectReferencedAttachmentIds(
    [...descriptionRows, ...commentRows].map((row) => row.text ?? ``),
    origin
  )
  for (const row of commentLinkedRows) {
    referenced.add(row.id)
  }
  return referenced
}

export const attachmentsRouter = router({
  /**
   * Manual attachment deletion (EXP-297 replaced the automatic GC). Any team
   * member may delete; every markdown reference to the row — in this team's
   * issue descriptions and comment bodies — is rewritten to a plain-text
   * placeholder in the SAME transaction, so no body is ever left pointing at
   * a dead attachment (which the issues.update round-trip guard would reject).
   */
  delete: authedProcedure
    // Lowercased so the markdown rewrite (which compares against lowercase ids
    // extracted from stored URLs) can never silently miss on an uppercase
    // caller-supplied uuid (e.g. from an MCP agent).
    .input(
      z.object({ id: z.string().uuid().transform((value) => value.toLowerCase()) })
    )
    .mutation(async ({ ctx, input }) => {
      // Resolved from the attachments row itself (NOT the trash-excluding
      // board join): listForTeam shows trashed-board rows, and owners must be
      // able to reclaim them instead of dead-ending on "not found".
      const [membershipRow] = await ctx.db
        .select({ teamId: attachments.teamId })
        .from(attachments)
        .where(eq(attachments.id, input.id))
        .limit(1)

      if (!membershipRow) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Attachment not found`,
        })
      }

      await assertTeamMember(ctx.session.user.id, membershipRow.teamId)

      const origin = ctx.request.url

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        const [row] = await tx
          .select({
            id: attachments.id,
            filename: attachments.filename,
            storageKey: attachments.storageKey,
            teamId: attachments.teamId,
          })
          .from(attachments)
          .where(eq(attachments.id, input.id))
          .limit(1)

        if (!row) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Attachment not found`,
          })
        }

        await replaceAttachmentReferencesInTx(tx, {
          targets: [{ id: input.id, filename: row.filename }],
          teamId: row.teamId,
          origin,
        })

        await tx.delete(attachments).where(eq(attachments.id, input.id))

        return { txId, storageKey: row.storageKey }
      })

      // Blob reclamation happens only after the row is really gone.
      await deleteStorageObjects([result.storageKey])

      return { txId: result.txId }
    }),

  /**
   * Owner-only storage manager feed: every attachment row of the team with a
   * `referenced` flag (does any issue description / comment body still embed
   * it?) and the team's total attachment bytes.
   */
  listForTeam: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)

      const rows = await ctx.db
        .select({
          id: attachments.id,
          issueId: attachments.issueId,
          boardId: attachments.boardId,
          uploaderId: attachments.uploaderId,
          filename: attachments.filename,
          contentType: attachments.contentType,
          sizeBytes: attachments.sizeBytes,
          width: attachments.width,
          height: attachments.height,
          createdAt: attachments.createdAt,
        })
        .from(attachments)
        .where(eq(attachments.teamId, input.teamId))
        .orderBy(desc(attachments.createdAt))

      const referencedIds = await collectTeamReferencedAttachmentIdsInTx(
        ctx.db,
        input.teamId,
        ctx.request.url
      )

      return {
        attachments: rows.map((row) => ({
          ...row,
          isImage: isAcceptedImageContentType(row.contentType),
          referenced: referencedIds.has(row.id),
        })),
        totalBytes: rows.reduce((total, row) => total + row.sizeBytes, 0),
      }
    }),

  /**
   * Owner-only bulk reclaim: deletes IMAGE attachments that no markdown in the
   * team references any more and that are older than the grace window. Files
   * (non-inline-image rows) are never swept — they live in the issue's Files
   * list, which is not a markdown reference.
   */
  sweepUnreferencedImages: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)

      const cutoff = new Date(Date.now() - SWEEP_GRACE_MS)
      const origin = ctx.request.url

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        const rows = await tx
          .select({
            id: attachments.id,
            contentType: attachments.contentType,
            sizeBytes: attachments.sizeBytes,
            storageKey: attachments.storageKey,
            createdAt: attachments.createdAt,
          })
          .from(attachments)
          .where(eq(attachments.teamId, input.teamId))

        const imageRows = rows.filter((row) =>
          isAcceptedImageContentType(row.contentType)
        )

        if (imageRows.length === 0) {
          return { txId, deleted: [], skippedRecentCount: 0 }
        }

        const referencedIds = await collectTeamReferencedAttachmentIdsInTx(
          tx,
          input.teamId,
          origin
        )

        const unreferenced = imageRows.filter(
          (row) => !referencedIds.has(row.id)
        )
        const deleted = unreferenced.filter((row) => row.createdAt < cutoff)
        const skippedRecentCount = unreferenced.length - deleted.length

        if (deleted.length > 0) {
          await tx.delete(attachments).where(
            inArray(
              attachments.id,
              deleted.map((row) => row.id)
            )
          )
        }

        return { txId, deleted, skippedRecentCount }
      })

      await deleteStorageObjects(result.deleted.map((row) => row.storageKey))

      return {
        txId: result.txId,
        deletedCount: result.deleted.length,
        freedBytes: result.deleted.reduce(
          (total, row) => total + row.sizeBytes,
          0
        ),
        skippedRecentCount: result.skippedRecentCount,
      }
    }),
})
