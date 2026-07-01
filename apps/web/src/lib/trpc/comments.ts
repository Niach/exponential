import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { eq } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { comments } from "@/db/schema"
import { commentBodySchema, getCommentBodyText } from "@/lib/domain"
import {
  resolveWorkspaceAccess,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"
import { isAgentUser } from "@/lib/auth/app-user"
import { fireAndForgetCommentNotify } from "@/lib/integrations/notifications"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import { resolveMentions } from "@/lib/integrations/mentions"

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
      workspaceId: comments.workspaceId,
    })
    .from(comments)
    .where(eq(comments.id, commentId))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Comment not found` })
  }
  return row
}

export const commentsRouter = router({
  create: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        body: commentBodySchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await getIssueWorkspaceContext(input.issueId)
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        issueContext.workspaceId,
        `comment`
      )

      // The widget-helpdesk bot (users.isAgent) authors issues, not threaded
      // comments — but guard anyway so a bot-authored comment never fans out.
      const isBotAuthor = isAgentUser(ctx.session.user)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [comment] = await tx
          .insert(comments)
          .values({
            issueId: input.issueId,
            workspaceId: issueContext.workspaceId,
            authorId: ctx.session.user.id,
            body: input.body,
          })
          .returning()

        // Auto-subscribe the commenter (skipped for agents inside ensureSubscribed).
        await ensureSubscribed(tx, {
          issueId: input.issueId,
          userId: ctx.session.user.id,
          workspaceId: issueContext.workspaceId,
          source: `commenter`,
        })

        // Resolve @email mentions to workspace members and auto-subscribe them
        // (source='mention') so they keep following the thread.
        const mentionedUserIds = await resolveMentions(
          tx,
          getCommentBodyText(input.body),
          issueContext.workspaceId
        )
        for (const userId of mentionedUserIds) {
          await ensureSubscribed(tx, {
            issueId: input.issueId,
            userId,
            workspaceId: issueContext.workspaceId,
            source: `mention`,
          })
        }

        return { txId, comment, mentionedUserIds }
      })

      // Bot-authored comments never fan out as notifications.
      if (!isBotAuthor) {
        fireAndForgetCommentNotify({
          issueId: input.issueId,
          actorUserId: ctx.session.user.id,
          commentBodyText: getCommentBodyText(input.body),
          mentionedUserIds: result.mentionedUserIds,
        })
      }

      return result
    }),

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        body: commentBodySchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadCommentForMutation(ctx.db, input.id)
      const isAuthor = existing.authorId === ctx.session.user.id
      const isAdmin = await isUserAdmin(ctx.session.user.id)
      if (!isAuthor && !isAdmin) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the author can edit this comment`,
        })
      }
      if (!isAdmin) {
        // Authorship alone isn't enough: the author must still have comment
        // access to the workspace (membership, or any-authed-user on a public
        // workspace). Blocks edits by authors who since left a private
        // workspace. Global admins keep their bypass.
        await resolveWorkspaceAccess(
          ctx.session.user.id,
          existing.workspaceId,
          `comment`
        )
      }

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [comment] = await tx
          .update(comments)
          .set({ body: input.body, editedAt: new Date() })
          .where(eq(comments.id, input.id))
          .returning()
        return { txId, comment }
      })

      return result
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadCommentForMutation(ctx.db, input.id)
      const isAuthor = existing.authorId === ctx.session.user.id
      const isAdmin = await isUserAdmin(ctx.session.user.id)
      if (!isAuthor && !isAdmin) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the author or an admin can delete this comment`,
        })
      }
      if (!isAdmin) {
        // Same workspace-access gate as update (see comment there); global
        // admins keep their bypass.
        await resolveWorkspaceAccess(
          ctx.session.user.id,
          existing.workspaceId,
          `comment`
        )
      }

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(comments).where(eq(comments.id, input.id))
        return { txId }
      })

      return result
    }),
})

