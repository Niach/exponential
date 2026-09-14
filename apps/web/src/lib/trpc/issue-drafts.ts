import { z } from "zod"
import { and, eq, inArray } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, issueDrafts, issueStatuses } from "@/db/schema"
import { getBoardTeamId, resolveTeamAccess } from "@/lib/team-membership"
import {
  dateOnlySchema,
  issueDescriptionSchema,
  issuePrioritySchema,
} from "@/lib/domain"
import {
  canonicalizeMarkdownImageUrls,
  collectAttachmentStorageKeys,
  extractAttachmentIdsFromDescription,
} from "@/lib/storage/issue-attachments"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"

// EXP-878: issue DRAFTS — what the create-issue dialog keeps when it is
// closed with content in it, on every client. Three rules shape this router:
//
//  * The row's id is CLIENT-MINTED and the write is an UPSERT, so a dialog
//    session owns exactly one row no matter how often it is closed and
//    reopened, and `issues.create({ draftId })` can delete it in the same
//    transaction it creates the issue in.
//  * Everything is OWNER-scoped: a draft is private to the person composing
//    it, never visible to teammates (the shape is `user_id = me`). `delete`
//    and `listAttachments` therefore need no team lookup at all — the
//    `user_id` predicate IS the authorization.
//  * Uploads are EAGER (the dialog uploads a pasted image straight away), so
//    a description reaching this router already carries FINAL
//    `/api/attachments/{id}` URLs, and every one of them must belong to THIS
//    draft. That is the same round-trip guard `issues.update` applies, moved
//    one step earlier.

const draftIdSchema = z.string().uuid()

export const issueDraftsRouter = router({
  /**
   * Create or update the caller's draft. ONE idempotent write per dialog
   * close (never per keystroke): the client mints the id when it opens the
   * dialog blank and reuses the row's id when it reopens one.
   */
  upsert: authedProcedure
    .input(
      z.object({
        id: draftIdSchema,
        teamId: z.string().uuid(),
        boardId: z.string().uuid(),
        title: z.string().max(500).default(``),
        description: issueDescriptionSchema.default(``),
        statusId: z.string().uuid().nullish(),
        priority: issuePrioritySchema.default(`none`),
        assigneeId: z.string().nullish(),
        labelIds: z.array(z.string().uuid()).max(50).default([]),
        dueDate: dateOnlySchema.nullish(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      // boardVisible() lives inside getBoardTeamId: a draft can never be
      // filed onto a trashed or archived board.
      const board = await getBoardTeamId(input.boardId)
      if (board.teamId !== input.teamId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `The board must belong to this team`,
        })
      }
      // A draft is a pre-issue: the capability that gates it is the one that
      // gates the issue it will become.
      await resolveTeamAccess(userId, input.teamId, `create_issue`)

      if (input.statusId) {
        const [statusRow] = await ctx.db
          .select({ teamId: issueStatuses.teamId })
          .from(issueStatuses)
          .where(eq(issueStatuses.id, input.statusId))
          .limit(1)
        if (!statusRow || statusRow.teamId !== input.teamId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `The status must belong to this team`,
          })
        }
      }

      const origin = ctx.request.url
      const description = input.description ?? ``

      // Uploads are EAGER, so any image here must already be a draft-owned
      // attachment. Anything else (a blob: URL, a foreign host, another
      // issue's attachment) is refused rather than stored — a body pointing
      // at bytes this draft does not own would survive the reparenting step
      // as a permanently broken image.
      const { attachmentIds, invalidUrls } = extractAttachmentIdsFromDescription(
        description,
        origin
      )
      if (invalidUrls.length > 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Draft descriptions can only reference images uploaded to this draft`,
        })
      }
      if (attachmentIds.length > 0) {
        const rows = await ctx.db
          .select({ id: attachments.id })
          .from(attachments)
          .where(
            // The id list is bounded by the description itself; `uuid`
            // comparison is case-insensitive in Postgres, so an uppercase id
            // in the markdown still matches its row.
            and(
              eq(attachments.draftId, input.id),
              inArray(attachments.id, attachmentIds)
            )
          )
        if (rows.length !== attachmentIds.length) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Draft descriptions can only reference images uploaded to this draft`,
          })
        }
      }

      const canonicalDescription = canonicalizeMarkdownImageUrls(
        description,
        origin
      )

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const values = {
          id: input.id,
          userId,
          teamId: input.teamId,
          boardId: input.boardId,
          title: input.title ?? ``,
          description: canonicalDescription,
          statusId: input.statusId ?? null,
          priority: input.priority ?? (`none` as const),
          assigneeId: input.assigneeId ?? null,
          labelIds: input.labelIds ?? [],
          dueDate: input.dueDate ?? null,
        }
        const [draft] = await tx
          .insert(issueDrafts)
          .values(values)
          .onConflictDoUpdate({
            target: issueDrafts.id,
            set: {
              teamId: values.teamId,
              boardId: values.boardId,
              title: values.title,
              description: values.description,
              statusId: values.statusId,
              priority: values.priority,
              assigneeId: values.assigneeId,
              labelIds: values.labelIds,
              dueDate: values.dueDate,
            },
            // The id is client-minted, so a collision with SOMEBODY ELSE's
            // row is reachable (by guess or by malice). The setWhere makes
            // that update touch nothing at all; the empty `returning` below
            // turns it into a refusal instead of a silent no-op.
            setWhere: eq(issueDrafts.userId, userId),
          })
          .returning()

        if (!draft) {
          throw new TRPCError({
            code: `FORBIDDEN`,
            message: `This draft belongs to someone else`,
          })
        }

        return { draft, txId }
      })
    }),

  /**
   * Discard a draft. Owner-only by predicate (no team lookup): the row and
   * its attachment rows go in one transaction, the blobs after it commits —
   * the same order every other reclaim path uses, so a rolled-back delete can
   * never strand a live row pointing at deleted bytes.
   */
  delete: authedProcedure
    .input(z.object({ id: draftIdSchema }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        // Collected BEFORE the delete — the FK cascade drops the attachment
        // rows and never touches S3.
        const attachmentRows = await tx
          .select({
            storageKey: attachments.storageKey,
            posterStorageKey: attachments.posterStorageKey,
          })
          .from(attachments)
          .where(eq(attachments.draftId, input.id))

        const deleted = await tx
          .delete(issueDrafts)
          .where(
            and(eq(issueDrafts.id, input.id), eq(issueDrafts.userId, userId))
          )
          .returning({ id: issueDrafts.id })

        return {
          txId,
          deleted: deleted.length > 0,
          storageKeys:
            deleted.length > 0
              ? collectAttachmentStorageKeys(attachmentRows)
              : [],
        }
      })

      await deleteStorageObjects(result.storageKeys)

      return { txId: result.txId, deleted: result.deleted }
    }),

  /**
   * The draft's attachment rows — the create dialog's Files rail when a draft
   * is reopened. Draft attachments are deliberately NOT synced (the
   * attachments shape filters `issue_id IS NOT NULL`), so this is the only
   * way to see them. Owner-only.
   */
  listAttachments: authedProcedure
    .input(z.object({ id: draftIdSchema }))
    .query(async ({ ctx, input }) => {
      const userId = ctx.session.user.id

      const [draft] = await ctx.db
        .select({ id: issueDrafts.id })
        .from(issueDrafts)
        .where(and(eq(issueDrafts.id, input.id), eq(issueDrafts.userId, userId)))
        .limit(1)

      if (!draft) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Draft not found`,
        })
      }

      return await ctx.db
        .select({
          id: attachments.id,
          filename: attachments.filename,
          contentType: attachments.contentType,
          sizeBytes: attachments.sizeBytes,
          url: attachments.url,
          width: attachments.width,
          height: attachments.height,
          durationMs: attachments.durationMs,
          createdAt: attachments.createdAt,
        })
        .from(attachments)
        .where(eq(attachments.draftId, input.id))
    }),
})
