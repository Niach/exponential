import { z } from "zod"
import { and, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { fcmTokens } from "@/db/schema"

export const pushTokensRouter = router({
  register: authedProcedure
    .input(
      z.object({
        token: z.string().min(1),
        platform: z.enum([`android`, `ios`, `web`]),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      // Steal the token from any other user it was previously registered to —
      // happens when a phone signs out and another user signs in.
      await ctx.db
        .insert(fcmTokens)
        .values({
          userId,
          token: input.token,
          platform: input.platform,
        })
        .onConflictDoUpdate({
          target: fcmTokens.token,
          set: { userId, platform: input.platform, updatedAt: new Date() },
        })
      return { ok: true }
    }),

  unregister: authedProcedure
    .input(z.object({ token: z.string().min(1) }))
    .mutation(async ({ ctx, input }) => {
      await ctx.db
        .delete(fcmTokens)
        .where(
          and(
            eq(fcmTokens.token, input.token),
            eq(fcmTokens.userId, ctx.session.user.id)
          )
        )
      return { ok: true }
    }),
})
