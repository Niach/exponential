import { z } from "zod"
import { and, asc, eq, gt, isNull } from "drizzle-orm"
import { authedProcedure } from "@/lib/trpc"
import { issues, agentRegistrations } from "@/db/schema"
import { revokeDeviceAgent } from "@/lib/companion-agents"
import { loadAgentForSessionUser } from "./shared"

export const hubProcedures = {
  heartbeat: authedProcedure.mutation(async ({ ctx }) => {
    const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
    const [updated] = await ctx.db
      .update(agentRegistrations)
      .set({ lastSeenAt: new Date() })
      .where(eq(agentRegistrations.id, agent.id))
      .returning({ lastSeenAt: agentRegistrations.lastSeenAt })

    return { ok: true, lastSeenAt: updated?.lastSeenAt ?? null }
  }),

  // `mutation` instead of `query` because the daemon's tRPC client uses POST
  // for everything, and we still want to update `lastSeenAt` on each poll —
  // so a write-shaped semantics fits.
  //
  // `activityCursor` is the daemon's last-seen `issues.updated_at` timestamp.
  // We return any issues currently assigned to this agent that have been
  // updated since the cursor, plus a new cursor value. This is the fallback
  // path for new comments / row updates when the Electric ShapeStream isn't
  // delivering live events (e.g., due to proxy long-poll header stripping).
  pollControl: authedProcedure
    .input(
      z
        .object({
          activityCursor: z.string().datetime().optional(),
        })
        .optional()
    )
    .mutation(async ({ ctx, input }) => {
      const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
      const now = new Date()
      await ctx.db
        .update(agentRegistrations)
        .set({ lastSeenAt: now })
        .where(eq(agentRegistrations.id, agent.id))

      let activityIssues: Array<{
        id: string
        identifier: string
        title: string
        projectId: string
        assigneeId: string | null
        updatedAt: string
      }> = []
      let nextCursor = input?.activityCursor ?? now.toISOString()

      if (input?.activityCursor) {
        const since = new Date(input.activityCursor)
        const rows = await ctx.db
          .select({
            id: issues.id,
            identifier: issues.identifier,
            title: issues.title,
            projectId: issues.projectId,
            assigneeId: issues.assigneeId,
            archivedAt: issues.archivedAt,
            updatedAt: issues.updatedAt,
          })
          .from(issues)
          .where(
            and(
              eq(issues.assigneeId, agent.userId),
              isNull(issues.archivedAt),
              gt(issues.updatedAt, since)
            )
          )
          .orderBy(asc(issues.updatedAt))
          .limit(50)

        activityIssues = rows.map((row) => ({
          id: row.id,
          identifier: row.identifier,
          title: row.title,
          projectId: row.projectId,
          assigneeId: row.assigneeId,
          updatedAt: row.updatedAt.toISOString(),
        }))

        const lastRow = rows.at(-1)
        if (lastRow) {
          nextCursor = lastRow.updatedAt.toISOString()
        }
      }

      return {
        activity: {
          cursor: nextCursor,
          issues: activityIssues,
        },
      }
    }),

  uninstallSelf: authedProcedure.mutation(async ({ ctx }) => {
    const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
    await revokeDeviceAgent(ctx.db, agent)
    return { ok: true }
  }),
}
