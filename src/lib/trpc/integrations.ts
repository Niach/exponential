import { and, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { accounts } from "@/db/schema"

const GOOGLE_PROVIDER_ID = `google`

export const integrationsRouter = router({
  google: router({
    status: authedProcedure.query(async ({ ctx }) => {
      const [account] = await ctx.db
        .select({
          accountId: accounts.accountId,
          scope: accounts.scope,
          createdAt: accounts.createdAt,
        })
        .from(accounts)
        .where(
          and(
            eq(accounts.userId, ctx.session.user.id),
            eq(accounts.providerId, GOOGLE_PROVIDER_ID)
          )
        )
        .limit(1)

      if (!account) {
        return { connected: false as const }
      }

      return {
        connected: true as const,
        scope: account.scope,
        connectedAt: account.createdAt,
      }
    }),

    disconnect: authedProcedure.mutation(async ({ ctx }) => {
      await ctx.db
        .delete(accounts)
        .where(
          and(
            eq(accounts.userId, ctx.session.user.id),
            eq(accounts.providerId, GOOGLE_PROVIDER_ID)
          )
        )
      return { ok: true as const }
    }),
  }),
})
