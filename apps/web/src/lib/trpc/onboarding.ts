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
})
