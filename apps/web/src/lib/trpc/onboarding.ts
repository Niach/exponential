import { z } from "zod"
import { eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { users } from "@/db/schema"

export const onboardingRouter = router({
  complete: authedProcedure.mutation(async ({ ctx }) => {
    await ctx.db
      .update(users)
      .set({ onboardingCompletedAt: new Date(), updatedAt: new Date() })
      .where(eq(users.id, ctx.session.user.id))
    return { ok: true }
  }),

  // Persist the explicit dismissal of the setup checklist so it stays hidden
  // across web + desktop. Set `dismissed: false` to bring it back.
  dismissSetupChecklist: authedProcedure
    .input(z.object({ dismissed: z.boolean().default(true) }))
    .mutation(async ({ ctx, input }) => {
      await ctx.db
        .update(users)
        .set({
          setupChecklistDismissedAt: input.dismissed ? new Date() : null,
          updatedAt: new Date(),
        })
        .where(eq(users.id, ctx.session.user.id))
      return { ok: true }
    }),
})
