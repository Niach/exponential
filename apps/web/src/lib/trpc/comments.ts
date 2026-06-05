import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { eq } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { comments } from "@/db/schema"
import {
  commentBodySchema,
  commentKindSchema,
  getCommentBodyText,
} from "@/lib/domain"
import {
  assertCanCommentInWorkspace,
  getIssueWorkspaceContext,
  getWorkspaceMember,
} from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"
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
        kind: commentKindSchema.default(`regular`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await getIssueWorkspaceContext(input.issueId)
      await assertCanCommentInWorkspace(
        ctx.session.user.id,
        issueContext.workspaceId
      )

      const member = await getWorkspaceMember(
        ctx.session.user.id,
        issueContext.workspaceId
      )
      const isAgentAuthor = member?.role === `agent`

      // Only `regular` comments exist now — agent plan/question lifecycle lives in
      // the structured issue_agent_state store, not in comments. `input.kind` is
      // already constrained to `regular` by commentKindSchema.

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [comment] = await tx
          .insert(comments)
          .values({
            issueId: input.issueId,
            workspaceId: issueContext.workspaceId,
            authorId: ctx.session.user.id,
            body: input.body,
            kind: input.kind,
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

      // Agent-authored comments never fan out as notifications — agent
      // action-needed alerts go through fireAndForgetAgentActionNotify instead.
      if (!isAgentAuthor) {
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
      if (!isAuthor && !(await isUserAdmin(ctx.session.user.id))) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the author can edit this comment`,
        })
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
      if (!isAuthor && !(await isUserAdmin(ctx.session.user.id))) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the author or an admin can delete this comment`,
        })
      }

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(comments).where(eq(comments.id, input.id))
        return { txId }
      })

      return result
    }),
})

