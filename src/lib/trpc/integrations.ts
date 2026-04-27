import { and, eq, inArray, isNotNull, isNull } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { accounts, issues, projects } from "@/db/schema"
import { fireAndForgetSync } from "@/lib/google-calendar"
import { getUserWorkspaceIds } from "@/lib/workspace-membership"

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

    /**
     * Backfill: sync every issue with a due date that the user can see and
     * doesn't yet have a calendar event. Called once after the user freshly
     * links Google so existing issues land in their calendar without
     * needing to be re-edited.
     */
    backfill: authedProcedure.mutation(async ({ ctx }) => {
      const userId = ctx.session.user.id
      const workspaceIds = await getUserWorkspaceIds(userId)
      if (workspaceIds.length === 0) {
        return { ok: true as const, scheduled: 0 }
      }

      const projectRows = await ctx.db
        .select({ id: projects.id })
        .from(projects)
        .where(inArray(projects.workspaceId, workspaceIds))
      const projectIds = projectRows.map((r) => r.id)
      if (projectIds.length === 0) {
        return { ok: true as const, scheduled: 0 }
      }

      const candidates = await ctx.db
        .select()
        .from(issues)
        .where(
          and(
            inArray(issues.projectId, projectIds),
            isNotNull(issues.dueDate),
            isNull(issues.googleCalendarEventId),
            isNull(issues.archivedAt)
          )
        )

      for (const issue of candidates) {
        if (issue.status === `done` || issue.status === `cancelled`) continue
        fireAndForgetSync(userId, issue)
      }

      return { ok: true as const, scheduled: candidates.length }
    }),
  }),
})
