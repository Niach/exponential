import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { and, desc, eq, inArray, isNull } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, issueDrafts } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import {
  collectAttachmentStorageKeys,
  isAcceptedImageContentType,
  isAudioContentType,
  isVideoContentType,
} from "@/lib/storage/issue-attachments"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { collectSweepableIdsInTx, SWEEP_COLUMNS } from "@/lib/storage/image-references"
import { replaceAttachmentReferencesInTx } from "@/lib/storage/attachment-references"

// Grace window for the owner-triggered sweep: an image uploaded moments ago
// may still be sitting in an unsaved editor draft, so never reclaim anything
// younger than this even when no markdown references it yet.
export const SWEEP_GRACE_MS = 24 * 60 * 60 * 1000

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
        .select({ teamId: attachments.teamId, draftId: attachments.draftId })
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

      // EXP-878: a draft's attachment belongs to the draft's author alone,
      // like the draft itself (`issueDrafts.listAttachments`).
      if (membershipRow.draftId) {
        const [draft] = await ctx.db
          .select({ id: issueDrafts.id })
          .from(issueDrafts)
          .where(
            and(
              eq(issueDrafts.id, membershipRow.draftId),
              eq(issueDrafts.userId, ctx.session.user.id)
            )
          )
          .limit(1)
        if (!draft) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Attachment not found`,
          })
        }
      }

      const origin = ctx.request.url

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        const [row] = await tx
          .select({
            id: attachments.id,
            filename: attachments.filename,
            storageKey: attachments.storageKey,
            posterStorageKey: attachments.posterStorageKey,
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

        return { txId, storageKeys: collectAttachmentStorageKeys([row]) }
      })

      // Blob reclamation happens only after the row is really gone.
      await deleteStorageObjects(result.storageKeys)

      return { txId: result.txId }
    }),

  /**
   * Owner-only storage manager feed: every ISSUE-owned attachment row of the
   * team with a `referenced` flag (does any issue description / comment body
   * still embed it?) and the team's total attachment bytes.
   *
   * EXP-878: draft-owned rows (`issue_id`/`board_id` NULL) are NOT listed.
   * Every deployed desktop decodes `issueId`/`boardId` as non-null strings
   * and rejects the WHOLE response on one null, and a draft's files are
   * private to its author anyway (`issueDrafts.listAttachments`). Their bytes
   * still count: `totalBytes` covers the team's every row so the usage
   * number stays honest, and `draftBytes` names the part the list omits
   * (swept images waiting for their purge too).
   */
  listForTeam: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)

      const allRows = await ctx.db
        .select({
          ...SWEEP_COLUMNS,
          uploaderId: attachments.uploaderId,
          filename: attachments.filename,
          sizeBytes: attachments.sizeBytes,
          width: attachments.width,
          height: attachments.height,
          durationMs: attachments.durationMs,
          posterStorageKey: attachments.posterStorageKey,
          createdAt: attachments.createdAt,
          sweptAt: attachments.sweptAt,
        })
        .from(attachments)
        .where(eq(attachments.teamId, input.teamId))
        .orderBy(desc(attachments.createdAt))

      // Partitioned here rather than in SQL so ONE query feeds both the list
      // and the honest total; the type guard is what keeps the row contract
      // (non-null `issueId`/`boardId`) visible to the callers' types.
      const rows = allRows.filter(
        (row): row is typeof row & { issueId: string; boardId: string } =>
          row.issueId !== null && row.boardId !== null && row.sweptAt === null
      )

      const { referencedIds } = await collectSweepableIdsInTx(
        ctx.db,
        input.teamId,
        ctx.request.url,
        rows
      )

      const totalBytes = allRows.reduce(
        (total, row) => total + row.sizeBytes,
        0
      )
      const listedBytes = rows.reduce((total, row) => total + row.sizeBytes, 0)

      return {
        attachments: rows.map(
          ({ commentId: _c, draftId: _d, messageId: _m, appId: _a, sessionId, asFile: _f, sweptAt: _s, ...row }) => ({
          ...row,
          isImage: isAcceptedImageContentType(row.contentType),
          // EXP-824: inline media rows preview in the lightbox and show a
          // duration; `referenced` already counts their plain-link embeds.
          isVideo: isVideoContentType(row.contentType),
          isAudio: isAudioContentType(row.contentType),
          // A result filed on an issue is named by its session.
          referenced: sessionId !== null || referencedIds.has(row.id),
        })),
        totalBytes,
        draftBytes: totalBytes - listedBytes,
      }
    }),

  /**
   * Owner-only bulk reclaim: marks ISSUE images that nothing in the team
   * references any more (`isSweepableImage`, #EXP-2115) and that are older
   * than the grace window; the purge deletes a marked one 30 days later
   * (lib/swept-images.ts), unless something showed or named it by then. Files (non-inline-image rows, and EXP-1247
   * `as_file` images) are never swept — they live in the issue's Files list,
   * which is not a markdown reference.
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
            ...SWEEP_COLUMNS,
            sizeBytes: attachments.sizeBytes,
            createdAt: attachments.createdAt,
          })
          .from(attachments)
          .where(and(eq(attachments.teamId, input.teamId), isNull(attachments.sweptAt)))

        const { sweepableIds } = await collectSweepableIdsInTx(
          tx,
          input.teamId,
          origin,
          rows
        )
        const unreferenced = rows.filter((row) => sweepableIds.has(row.id))
        const deleted = unreferenced.filter((row) => row.createdAt < cutoff)
        const skippedRecentCount = unreferenced.length - deleted.length

        if (deleted.length > 0) {
          await tx
            .update(attachments)
            .set({ sweptAt: new Date() })
            .where(
              inArray(
                attachments.id,
                deleted.map((row) => row.id)
              )
            )
        }

        return { txId, deleted, skippedRecentCount }
      })

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
