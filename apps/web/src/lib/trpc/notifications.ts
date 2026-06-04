import { z } from "zod"
import { and, eq, isNull } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { notifications } from "@/db/schema"

// Inbox mark-read. Ownership-guarded on user_id so a caller can only touch their
// own rows. read_at updates re-stream over the per-user notifications shape.
export const notificationsRouter = router({
  markRead: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(notifications)
          .set({ readAt: new Date() })
          .where(
            and(
              eq(notifications.id, input.id),
              eq(notifications.userId, ctx.session.user.id)
            )
          )
        return { txId }
      })
    }),

  markAllRead: authedProcedure.mutation(async ({ ctx }) => {
    return await ctx.db.transaction(async (tx) => {
      const txId = await generateTxId(tx)
      await tx
        .update(notifications)
        .set({ readAt: new Date() })
        .where(
          and(
            eq(notifications.userId, ctx.session.user.id),
            isNull(notifications.readAt)
          )
        )
      return { txId }
    })
  }),
})
