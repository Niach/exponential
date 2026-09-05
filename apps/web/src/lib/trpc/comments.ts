import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { eq, inArray } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, comments } from "@/db/schema"
import {
  commentBodyWithAttachmentsSchema,
  getCommentBodyText,
  MAX_COMMENT_ATTACHMENTS,
} from "@/lib/domain"
import { resolveTeamAccess, getIssueTeamContext } from "@/lib/team-membership"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { replaceAttachmentReferencesInTx } from "@/lib/storage/attachment-references"
import {
  fireAndForgetCommentNotify,
  fireAndForgetIssueMentionNotify,
} from "@/lib/integrations/notifications"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import { resolveMentions } from "@/lib/integrations/mentions"
import { syncReferenceRelations } from "@/lib/issue-relations"

async function loadCommentForMutation(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  commentId: string
) {
  const [row] = await db
    .select({
      id: comments.id,
      authorId: comments.authorId,
      issueId: comments.issueId,
      teamId: comments.teamId,
    })
    .from(comments)
    .where(eq(comments.id, commentId))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Comment not found` })
  }
  return row
}

/**
 * EXP-741: resolve the comment a reply hangs off. Threads are ONE level deep:
 * the parent must be a comment on the same issue, and a reply to a reply is
 * re-parented onto that reply's own parent, so every reply renders under a
 * top-level card on every client (the natives only offer the reply row on
 * top-level cards; MCP callers may name any comment id).
 */
async function resolveReplyParent(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  parentId: string,
  issueId: string
): Promise<string> {
  const [parent] = await db
    .select({
      id: comments.id,
      issueId: comments.issueId,
      parentId: comments.parentId,
    })
    .from(comments)
    .where(eq(comments.id, parentId))
    .limit(1)
  if (!parent) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Comment not found` })
  }
  if (parent.issueId !== issueId) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `A reply must answer a comment on the same issue`,
    })
  }
  return parent.parentId ?? parent.id
}

type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

/**
 * Reconcile a comment's linked attachments to exactly `attachmentIds`
 * (EXP-554: comment attachments ride attachments.comment_id, never inline
 * markdown). Additions must belong to the SAME issue, be uploaded by the
 * comment author, and not be claimed by another comment — the uploader gate is
 * what makes the hard delete below safe: nobody can capture a teammate's
 * upload into their comment and then destroy it. Rows previously linked but
 * absent from the new list are hard-deleted (a comment attachment exists for
 * its comment; unlinking would strand an orphan in the issue's Files rail).
 * Returns their storage keys for post-commit blob reclamation.
 */
async function syncCommentAttachmentsInTx(
  tx: Tx,
  args: {
    commentId: string
    issueId: string
    teamId: string
    userId: string
    origin: string
    attachmentIds: string[]
  }
): Promise<{ deletedStorageKeys: string[] }> {
  const requested = [...new Set(args.attachmentIds)]

  const rows =
    requested.length > 0
      ? await tx
          .select({
            id: attachments.id,
            issueId: attachments.issueId,
            commentId: attachments.commentId,
            uploaderId: attachments.uploaderId,
          })
          .from(attachments)
          .where(inArray(attachments.id, requested))
      : []

  const invalid =
    rows.length !== requested.length ||
    rows.some(
      (row) =>
        row.issueId !== args.issueId ||
        (row.commentId !== null && row.commentId !== args.commentId) ||
        row.uploaderId !== args.userId
    )
  if (invalid) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Comments can only reference attachments you uploaded to this issue`,
    })
  }

  const linked = await tx
    .select({
      id: attachments.id,
      filename: attachments.filename,
      storageKey: attachments.storageKey,
    })
    .from(attachments)
    .where(eq(attachments.commentId, args.commentId))

  const keep = new Set(requested)
  const linkedIds = new Set(linked.map((row) => row.id))
  const toAdd = requested.filter((id) => !linkedIds.has(id))
  const toRemove = linked.filter((row) => !keep.has(row.id))

  if (toAdd.length > 0) {
    await tx
      .update(attachments)
      .set({ commentId: args.commentId })
      .where(inArray(attachments.id, toAdd))
  }
  if (toRemove.length > 0) {
    // A comment attachment may ALSO be embedded inline in the issue
    // description or in an old-client comment body (the link gate only asks
    // for same issue + same uploader + unclaimed). Rewrite those references to
    // the deleted placeholder in this same tx, exactly like attachments.delete
    // does, or the next issues.update 400s on the round-trip guard.
    await replaceAttachmentReferencesInTx(tx, {
      targets: toRemove.map((row) => ({ id: row.id, filename: row.filename })),
      teamId: args.teamId,
      origin: args.origin,
    })
    await tx.delete(attachments).where(
      inArray(
        attachments.id,
        toRemove.map((row) => row.id)
      )
    )
  }

  return { deletedStorageKeys: toRemove.map((row) => row.storageKey) }
}

export const commentsRouter = router({
  create: authedProcedure
    .input(
      z
        .object({
          issueId: z.string().uuid(),
          body: commentBodyWithAttachmentsSchema,
          attachmentIds: z
            .array(z.string().uuid())
            .max(MAX_COMMENT_ATTACHMENTS)
            .default([]),
          // EXP-741: reply under this comment (see resolveReplyParent).
          parentId: z.string().uuid().optional(),
        })
        .superRefine((value, refineCtx) => {
          if (
            value.body.trim().length === 0 &&
            value.attachmentIds.length === 0
          ) {
            refineCtx.addIssue({
              code: `custom`,
              message: `Comment needs text or attachments`,
            })
          }
        })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await getIssueTeamContext(input.issueId)
      await resolveTeamAccess(
        ctx.session.user.id,
        issueContext.teamId,
        `comment`
      )
      const parentId = input.parentId
        ? await resolveReplyParent(ctx.db, input.parentId, input.issueId)
        : null

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [comment] = await tx
          .insert(comments)
          .values({
            issueId: input.issueId,
            teamId: issueContext.teamId,
            boardId: issueContext.boardId,
            authorId: ctx.session.user.id,
            parentId,
            // EXP-741: the MCP server's synthetic context is the ONLY thing
            // that marks a comment as agent-posted — never client input.
            source: ctx.viaMcp ? `mcp` : `user`,
            body: input.body,
          })
          .returning()

        // Auto-subscribe the commenter (skipped for agents inside ensureSubscribed).
        await ensureSubscribed(tx, {
          issueId: input.issueId,
          userId: ctx.session.user.id,
          teamId: issueContext.teamId,
          source: `commenter`,
        })

        // Resolve @email mentions to team members and auto-subscribe them
        // (source='mention') so they keep following the thread.
        const mentionedUserIds = await resolveMentions(
          tx,
          getCommentBodyText(input.body),
          issueContext.teamId
        )
        for (const userId of mentionedUserIds) {
          await ensureSubscribed(tx, {
            issueId: input.issueId,
            userId,
            teamId: issueContext.teamId,
            source: `mention`,
          })
        }

        // EXP-736: `#IDENT` tokens in a comment become `related` rows with
        // source='reference', exactly like the ones in a description. The new
        // comment IS the slot being replaced, so it is excluded from the
        // survivor scan and its text passed as nextText.
        await syncReferenceRelations(tx, {
          issueId: input.issueId,
          teamId: issueContext.teamId,
          actorUserId: ctx.session.user.id,
          previousText: ``,
          nextText: getCommentBodyText(input.body),
          excludeCommentId: comment.id,
        })

        if (input.attachmentIds.length > 0) {
          // Same tx (and txId) as the comment insert, so Electric delivers the
          // comment row and its linked attachment rows as one sync unit.
          await syncCommentAttachmentsInTx(tx, {
            commentId: comment.id,
            issueId: input.issueId,
            teamId: issueContext.teamId,
            userId: ctx.session.user.id,
            origin: ctx.request.url,
            attachmentIds: input.attachmentIds,
          })
        }

        return { txId, comment, mentionedUserIds }
      })

      fireAndForgetCommentNotify({
        issueId: input.issueId,
        actorUserId: ctx.session.user.id,
        commentBodyText: getCommentBodyText(input.body),
        mentionedUserIds: result.mentionedUserIds,
        attachmentCount: input.attachmentIds.length,
      })

      return result
    }),

  update: authedProcedure
    .input(
      z
        .object({
          id: z.string().uuid(),
          body: commentBodyWithAttachmentsSchema,
          // undefined = leave attachments untouched (MCP and old clients);
          // an array is the FULL desired set — missing linked rows are deleted.
          attachmentIds: z
            .array(z.string().uuid())
            .max(MAX_COMMENT_ATTACHMENTS)
            .optional(),
        })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadCommentForMutation(ctx.db, input.id)
      if (existing.authorId !== ctx.session.user.id) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the author can edit this comment`,
        })
      }
      // Authorship alone isn't enough: the author must still be a member of
      // the team. Blocks edits by authors who have since left. Global admins
      // have NO bypass here (EXP-398) — nobody rewrites or deletes someone
      // else's words, and every client hides the menu to match.
      await resolveTeamAccess(ctx.session.user.id, existing.teamId, `comment`)

      // "Body or attachments" checked HERE, not in a zod refine, so that an
      // empty body with attachmentIds omitted (= leave attachments alone)
      // counts the comment's EXISTING links (EXP-560 — a refine can't see
      // them).
      if (input.body.trim().length === 0) {
        const keptAttachments = input.attachmentIds
          ? input.attachmentIds.length
          : (
              await ctx.db
                .select({ id: attachments.id })
                .from(attachments)
                .where(eq(attachments.commentId, input.id))
                .limit(1)
            ).length
        if (keptAttachments === 0) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Comment needs text or attachments`,
          })
        }
      }

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [previous] = await tx
          .select({ body: comments.body })
          .from(comments)
          .where(eq(comments.id, input.id))
          .limit(1)
        const [comment] = await tx
          .update(comments)
          .set({ body: input.body, editedAt: new Date() })
          .where(eq(comments.id, input.id))
          .returning()

        // Comment @mentions, delta-based (mirrors the description edit path in
        // issues.update): only members mentioned in the NEW body but not the
        // old one are subscribed + notified, so re-saving an unchanged comment
        // never re-pings.
        const previouslyMentioned = new Set(
          await resolveMentions(
            tx,
            getCommentBodyText(previous?.body),
            existing.teamId
          )
        )
        const nextMentioned = await resolveMentions(
          tx,
          getCommentBodyText(input.body),
          existing.teamId
        )
        const newlyMentionedUserIds = nextMentioned.filter(
          (userId) => !previouslyMentioned.has(userId)
        )
        for (const userId of newlyMentionedUserIds) {
          await ensureSubscribed(tx, {
            issueId: existing.issueId,
            userId,
            teamId: existing.teamId,
            source: `mention`,
          })
        }

        // EXP-736: reference-relation delta for the edited body (same rules as
        // the description edit above).
        await syncReferenceRelations(tx, {
          issueId: existing.issueId,
          teamId: existing.teamId,
          actorUserId: ctx.session.user.id,
          previousText: getCommentBodyText(previous?.body),
          nextText: getCommentBodyText(input.body),
          excludeCommentId: input.id,
        })

        const { deletedStorageKeys } =
          input.attachmentIds !== undefined
            ? await syncCommentAttachmentsInTx(tx, {
                commentId: input.id,
                issueId: existing.issueId,
                teamId: existing.teamId,
                userId: ctx.session.user.id,
                origin: ctx.request.url,
                attachmentIds: input.attachmentIds,
              })
            : { deletedStorageKeys: [] }

        return { txId, comment, newlyMentionedUserIds, deletedStorageKeys }
      })

      // Blob reclamation only after the rows are really gone (same order as
      // attachments.delete).
      await deleteStorageObjects(result.deletedStorageKeys)

      // Mention-only fan-out: an edit is not a new comment, so subscribers
      // must not get an issue_comment ping (same reason issues.update uses it
      // for descriptions).
      if (result.newlyMentionedUserIds.length > 0) {
        fireAndForgetIssueMentionNotify({
          issueId: existing.issueId,
          actorUserId: ctx.session.user.id,
          mentionedUserIds: result.newlyMentionedUserIds,
        })
      }

      return { txId: result.txId, comment: result.comment }
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadCommentForMutation(ctx.db, input.id)
      if (existing.authorId !== ctx.session.user.id) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the author can delete this comment`,
        })
      }
      // Same author-only + still-a-member gate as update, no admin bypass
      // (see the comment there).
      await resolveTeamAccess(ctx.session.user.id, existing.teamId, `comment`)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        // EXP-741: a top-level comment takes its replies with it (the FK
        // cascades), so everything below runs over the whole thread — the
        // dying root plus its replies — not just the named row.
        const replies = await tx
          .select({
            id: comments.id,
            parentId: comments.parentId,
            body: comments.body,
          })
          .from(comments)
          .where(eq(comments.parentId, input.id))
        const dyingIds = [input.id, ...replies.map((row) => row.id)]
        // Comment attachments die with their comment (EXP-554). Collect and
        // delete BEFORE the comment row goes — the FK is ON DELETE SET NULL,
        // which would silently unlink them into the issue's Files rail.
        const linked = await tx
          .select({
            id: attachments.id,
            filename: attachments.filename,
            storageKey: attachments.storageKey,
          })
          .from(attachments)
          .where(inArray(attachments.commentId, dyingIds))
        if (linked.length > 0) {
          // Same reason as the unlink path in syncCommentAttachmentsInTx: a
          // linked row may still be embedded inline somewhere, so rewrite
          // those references to the placeholder before the row dies.
          await replaceAttachmentReferencesInTx(tx, {
            targets: linked.map((row) => ({
              id: row.id,
              filename: row.filename,
            })),
            teamId: existing.teamId,
            origin: ctx.request.url,
          })
          await tx
            .delete(attachments)
            .where(inArray(attachments.commentId, dyingIds))
        }
        const [dying] = await tx
          .select({ body: comments.body })
          .from(comments)
          .where(eq(comments.id, input.id))
          .limit(1)
        await tx.delete(comments).where(eq(comments.id, input.id))
        // EXP-736: a deleted comment takes its `#IDENT` references with it —
        // the reference rows only die when no other text still names them.
        // Replies are gone too by now (cascade), so each dying body is one
        // more previous→empty delta.
        for (const gone of [
          { id: input.id, body: dying?.body },
          ...replies,
        ]) {
          await syncReferenceRelations(tx, {
            issueId: existing.issueId,
            teamId: existing.teamId,
            actorUserId: ctx.session.user.id,
            previousText: getCommentBodyText(gone.body),
            nextText: ``,
            excludeCommentId: gone.id,
          })
        }
        return { txId, storageKeys: linked.map((row) => row.storageKey) }
      })

      await deleteStorageObjects(result.storageKeys)

      return { txId: result.txId }
    }),
})
