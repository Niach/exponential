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
      // One device token may be held by several signed-in accounts at once
      // (multi-account phones), so the conflict target is the (token, user)
      // pair: a re-register only refreshes this user's own row and never
      // steals the registration from another account on the same device.
      // Rows for accounts that sign out are removed by their own unregister
      // call; rows FCM invalidates are swept by the send path.
      await ctx.db
        .insert(fcmTokens)
        .values({
          userId,
          token: input.token,
          platform: input.platform,
        })
        .onConflictDoUpdate({
          target: [fcmTokens.token, fcmTokens.userId],
          set: { platform: input.platform, updatedAt: new Date() },
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
