import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { eq } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { comments } from "@/db/schema"
import { commentBodySchema, getCommentBodyText } from "@/lib/domain"
import {
  assertCanCommentInWorkspace,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"
import { fireAndForgetCommentNotify } from "@/lib/notifications"

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
      await assertCanCommentInWorkspace(
        ctx.session.user.id,
        issueContext.workspaceId
      )

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
        return { txId, comment }
      })

      fireAndForgetCommentNotify({
        issueId: input.issueId,
        actorUserId: ctx.session.user.id,
        commentBodyText: getCommentBodyText(input.body),
      })

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

