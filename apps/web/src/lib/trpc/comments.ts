import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { eq, inArray } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  attachments,
  comments,
  emailDeliveries,
  issues,
  teams,
  widgetSubmissions,
} from "@/db/schema"
import {
  commentAudienceSchema,
  commentBodyWithAttachmentsSchema,
  getCommentBodyText,
  MAX_COMMENT_ATTACHMENTS,
} from "@/lib/domain"
import { deliveryStatus, sendReporterReplyEmail } from "@/lib/email"
import { emailEnabled } from "@/lib/email-enabled"
import { mintReporterToken } from "@/lib/reporter/token"
import { reporterConversationUrl } from "@/lib/reporter/service"
import { resolveTeamAccess, getIssueTeamContext } from "@/lib/team-membership"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { replaceAttachmentReferencesInTx } from "@/lib/storage/attachment-references"
import { collectAttachmentStorageKeys } from "@/lib/storage/issue-attachments"
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

type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

/**
 * EXP-741: resolve the comment a reply hangs off. Threads are ONE level deep:
 * the parent must be a comment on the same issue, and a reply to a reply is
 * re-parented onto that reply's own parent, so every reply renders under a
 * top-level card on every client (the natives only offer the reply row on
 * top-level cards; MCP callers may name any comment id).
 *
 * Runs INSIDE the insert transaction: a parent deleted between the lookup and
 * the insert would otherwise fail the FK as an opaque 500 instead of the
 * NOT_FOUND the caller can act on.
 */
async function resolveReplyParent(
  tx: Tx,
  parentId: string,
  issueId: string
): Promise<string> {
  const [parent] = await tx
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
      posterStorageKey: attachments.posterStorageKey,
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

  return { deletedStorageKeys: collectAttachmentStorageKeys(toRemove) }
}

// SLOP-4: who a reporter-audience comment reaches — the issue's widget
// submission's email, plus what the email names (team + issue title).
interface ReporterRecipient {
  email: string
  teamName: string
  issueTitle: string
}

async function loadReporterRecipient(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  issueId: string
): Promise<ReporterRecipient | null> {
  const [row] = await db
    .select({
      email: widgetSubmissions.reporterEmail,
      teamName: teams.name,
      issueTitle: issues.title,
    })
    .from(widgetSubmissions)
    .innerJoin(issues, eq(issues.id, widgetSubmissions.issueId))
    .innerJoin(teams, eq(teams.id, issues.teamId))
    .where(eq(widgetSubmissions.issueId, issueId))
    .limit(1)
  if (!row?.email) return null
  return { email: row.email, teamName: row.teamName, issueTitle: row.issueTitle }
}

// Email a member's reporter-audience reply to the reporter and stamp the
// ledger row on the comment (audit). Never throws — the comment is already
// committed; the boolean is what the member's toast reports. With no mail
// transport nothing is sent or recorded (the schema's NULL case).
async function emailReporterReply(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  args: {
    commentId: string
    issueId: string
    reporter: ReporterRecipient
    replyText: string
  }
): Promise<boolean> {
  if (!emailEnabled) return false
  try {
    // The conversation URL embeds the magic-link token (recomputed, never
    // stored): the email carries it, the ledger row does not.
    const sendResult = await sendReporterReplyEmail({
      to: args.reporter.email,
      teamName: args.reporter.teamName,
      replyText: args.replyText,
      conversationUrl: reporterConversationUrl(mintReporterToken(args.issueId)),
    })
    const [ledger] = await db
      .insert(emailDeliveries)
      .values({
        userId: null,
        toEmail: args.reporter.email,
        issueId: args.issueId,
        kind: `reporter_reply`,
        status: deliveryStatus(sendResult),
        provider: sendResult.provider,
        providerMessageId: sendResult.messageId,
        subject: sendResult.subject,
        sentAt: sendResult.delivered ? new Date() : null,
      })
      .returning({ id: emailDeliveries.id })
    if (ledger) {
      await db
        .update(comments)
        .set({ emailDeliveryId: ledger.id })
        .where(eq(comments.id, args.commentId))
    }
    return sendResult.delivered
  } catch (error) {
    console.error(`reporter reply email failed`, error)
    return false
  }
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
          // SLOP-4: `reporter` = the words leave the team — shown on the
          // reporter's magic-link page and emailed to them. Top-level only,
          // and only on an issue whose widget submission has an email.
          audience: commentAudienceSchema.default(`team`),
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
          if (value.audience === `reporter` && value.parentId) {
            refineCtx.addIssue({
              code: `custom`,
              message: `A reply to the reporter must be a top-level comment`,
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
      // SLOP-4: a reporter-audience comment needs somebody to reach — the
      // issue's widget submission with a reporter email. Resolved up front
      // (outside the tx) so a plain team comment pays nothing for it.
      const reporter =
        input.audience === `reporter`
          ? await loadReporterRecipient(ctx.db, input.issueId)
          : null
      if (input.audience === `reporter` && !reporter) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This issue has no reporter email to reply to`,
        })
      }
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const parentId = input.parentId
          ? await resolveReplyParent(tx, input.parentId, input.issueId)
          : null
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
            audience: input.audience,
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

      // SLOP-4: the reporter-audience email, AFTER commit (the comment is
      // saved either way — the toast tells the member whether it went out).
      // `reporterEmailed` = null for team comments, true/false for reporter
      // ones (false = no transport, suppressed or failed).
      const reporterEmailed = reporter
        ? await emailReporterReply(ctx.db, {
            commentId: result.comment.id,
            issueId: input.issueId,
            reporter,
            replyText: getCommentBodyText(input.body),
          })
        : null

      return { ...result, reporterEmailed }
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
      // SLOP-4: a reporter's words (author_id NULL) are edited by nobody —
      // the check below would refuse them too, but the message should say
      // why.
      if (existing.authorId === null) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `A reporter's comment cannot be edited`,
        })
      }
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
      // SLOP-4: a reporter's comment (author_id NULL) has no author to own
      // it — any member may remove it (moderation of an anonymous visitor's
      // words). Everything else stays author-only.
      if (
        existing.authorId !== null &&
        existing.authorId !== ctx.session.user.id
      ) {
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
        // EXP-741 + EXP-398: a delete destroys the NAMED ROW AND NOTHING ELSE.
        // Replies survive their root — `comments.parent_id` is ON DELETE SET
        // NULL, so a teammate's reply flattens to a top-level card (every
        // client's thread helper already renders a reply with a missing parent
        // that way) instead of being deleted by someone else's delete. Their
        // attachments and their `#IDENT` references stay with them, so
        // everything below runs over this one comment only.
        const dyingIds = [input.id]
        // Comment attachments die with their comment (EXP-554). Collect and
        // delete BEFORE the comment row goes — the FK is ON DELETE SET NULL,
        // which would silently unlink them into the issue's Files rail.
        const linked = await tx
          .select({
            id: attachments.id,
            filename: attachments.filename,
            storageKey: attachments.storageKey,
            posterStorageKey: attachments.posterStorageKey,
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
        // Only THIS body is a previous→empty delta; surviving replies keep
        // their own references (and the scan below still sees their text).
        await syncReferenceRelations(tx, {
          issueId: existing.issueId,
          teamId: existing.teamId,
          actorUserId: ctx.session.user.id,
          previousText: getCommentBodyText(dying?.body),
          nextText: ``,
          excludeCommentId: input.id,
        })
        return { txId, storageKeys: collectAttachmentStorageKeys(linked) }
      })

      await deleteStorageObjects(result.storageKeys)

      return { txId: result.txId }
    }),
})
