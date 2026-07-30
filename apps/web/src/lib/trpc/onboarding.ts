import { eq, isNull, and } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { users } from "@/db/schema"
import { recordConversionEvent } from "@/lib/conversion/events"

export const onboardingRouter = router({
  complete: authedProcedure.mutation(async ({ ctx }) => {
    // The IS NULL guard makes the null→set transition observable, so the
    // conversion event fires exactly once per user — and never for the lazy
    // legacy backfill in lib/auth/onboarding.ts, which bypasses this router.
    const completed = await ctx.db
      .update(users)
      .set({ onboardingCompletedAt: new Date(), updatedAt: new Date() })
      .where(
        and(
          eq(users.id, ctx.session.user.id),
          isNull(users.onboardingCompletedAt)
        )
      )
      .returning({ id: users.id })
    if (completed.length > 0) {
      await recordConversionEvent(ctx.db, {
        name: `onboarding_completed`,
        userId: ctx.session.user.id,
      })
    }
    return { ok: true }
  }),
})
