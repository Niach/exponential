import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { codingSessions, releases } from "@/db/schema"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"

// The desktop launcher's live "coding now" record (§4a step 7). One row per
// interactive session; synced to every client as an Electric shape.
// Two subjects (EXP-56): issue-scoped (issueId) or release-scoped (releaseId —
// the desktop release orchestrator; issue_id/project_id stay NULL, the
// populate triggers no-op on NULL issue_id). Exactly one of the two.
// No generateTxId — native callers don't need the Electric tx-wait, and the
// row's own synced propagation carries the badge.
export const codingSessionsRouter = router({
  start: authedProcedure
    .input(
      z
        .object({
          issueId: z.string().uuid().optional(),
          releaseId: z.string().uuid().optional(),
          deviceLabel: z.string().max(255).optional(),
        })
        .refine((value) => Boolean(value.issueId) !== Boolean(value.releaseId), {
          message: `Exactly one of issueId/releaseId is required`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      if (input.issueId) {
        const issueCtx = await getIssueWorkspaceContext(input.issueId)
        await assertWorkspaceMember(ctx.session.user.id, issueCtx.workspaceId)

        const [session] = await ctx.db
          .insert(codingSessions)
          .values({
            issueId: input.issueId,
            // Set explicitly (also trigger-denormalized) so the row is valid even
            // if the populate_* triggers aren't applied.
            workspaceId: issueCtx.workspaceId,
            projectId: issueCtx.projectId,
            userId: ctx.session.user.id,
            deviceLabel: input.deviceLabel ?? null,
            status: `running`,
          })
          .returning()

        return { session }
      }

      const [release] = await ctx.db
        .select({ id: releases.id, workspaceId: releases.workspaceId })
        .from(releases)
        .where(eq(releases.id, input.releaseId!))
        .limit(1)
      if (!release) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Release not found`,
        })
      }
      await assertWorkspaceMember(ctx.session.user.id, release.workspaceId)

      const [session] = await ctx.db
        .insert(codingSessions)
        .values({
          releaseId: release.id,
          // workspace_id written directly from the release (no issue to
          // denormalize from); project_id stays NULL — a release run spans
          // projects and must never surface through the anonymous
          // project-scoped clause.
          workspaceId: release.workspaceId,
          userId: ctx.session.user.id,
          deviceLabel: input.deviceLabel ?? null,
          status: `running`,
        })
        .returning()

      return { session }
    }),

  // Liveness ping from the desktop while the claude child is alive. The
  // server-side staleness sweep treats a `running` row whose updated_at
  // stopped advancing as a crashed desktop and force-ends it — and flipping
  // the synced row to `ended` is exactly the desktop's remote-kill signal
  // (the own-row kill-switch tears the live child down on that transition),
  // so a genuinely-live session MUST keep its row fresh to survive the sweep.
  // Fire-and-forget on the client: a vanished row (issue/release cascade
  // delete) or an already-ended one is reported, never thrown.
  heartbeat: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [existing] = await ctx.db
        .select({
          userId: codingSessions.userId,
          status: codingSessions.status,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.id))
        .limit(1)

      if (!existing) return { alive: false }
      if (existing.userId !== ctx.session.user.id) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can heartbeat it`,
        })
      }
      if (existing.status !== `running`) return { alive: false }

      // Status-conditioned so a heartbeat racing a kill/end can never
      // resurrect the row's freshness after it ended.
      const updated = await ctx.db
        .update(codingSessions)
        .set({ updatedAt: new Date() })
        .where(
          and(
            eq(codingSessions.id, input.id),
            eq(codingSessions.status, `running`)
          )
        )
        .returning({ id: codingSessions.id })

      return { alive: updated.length > 0 }
    }),

  end: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [existing] = await ctx.db
        .select({
          id: codingSessions.id,
          userId: codingSessions.userId,
          status: codingSessions.status,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.id))
        .limit(1)

      if (!existing) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Coding session not found`,
        })
      }
      if (existing.userId !== ctx.session.user.id) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can end it`,
        })
      }

      // Idempotent: ending an already-ended session is a no-op.
      if (existing.status === `ended`) {
        const [row] = await ctx.db
          .select()
          .from(codingSessions)
          .where(eq(codingSessions.id, input.id))
          .limit(1)
        return { session: row }
      }

      const [session] = await ctx.db
        .update(codingSessions)
        .set({ status: `ended`, endedAt: new Date() })
        .where(eq(codingSessions.id, input.id))
        .returning()

      return { session }
    }),
})
