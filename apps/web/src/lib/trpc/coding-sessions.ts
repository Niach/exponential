import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { codingSessions } from "@/db/schema"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"

// The desktop launcher's live "coding now" record (§4a step 7). One row per
// interactive session; synced to every client as an Electric shape.
// Two subjects: issue-scoped (issueId) or batch-scoped (workspaceId — the
// desktop multi-issue batch orchestrator; issue_id/project_id stay NULL, the
// populate triggers no-op on NULL issue_id). Exactly one of the two.
// No generateTxId — native callers don't need the Electric tx-wait, and the
// row's own synced propagation carries the badge.
export const codingSessionsRouter = router({
  start: authedProcedure
    .input(
      z
        .object({
          issueId: z.string().uuid().optional(),
          workspaceId: z.string().uuid().optional(),
          deviceLabel: z.string().max(255).optional(),
        })
        .refine(
          (value) => Boolean(value.issueId) !== Boolean(value.workspaceId),
          {
            message: `Exactly one of issueId/workspaceId is required`,
          }
        )
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

      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId!)

      const [session] = await ctx.db
        .insert(codingSessions)
        .values({
          // Batch run: no issue to denormalize from — workspace_id written
          // directly; project_id stays NULL, a batch run spans projects and
          // must never surface through the anonymous project-scoped clause.
          workspaceId: input.workspaceId!,
          userId: ctx.session.user.id,
          deviceLabel: input.deviceLabel ?? null,
          status: `running`,
        })
        .returning()

      return { session }
    }),

  // Liveness ping from the desktop while the claude child is alive. The
  // server-side staleness sweep (lib/coding-session-sweep.ts) treats a
  // `running` row whose updated_at stopped advancing as a crashed desktop
  // and DELETES it — deliberately never flips it to `ended`, because that
  // transition is the desktop's remote-kill signal (a vanished row does not
  // fire the kill-switch), so deletion can never kill a live child.
  // A ping that finds its row GONE therefore means "swept while actually
  // alive" — a laptop suspend longer than CODING_SESSION_STALE_HOURS is the
  // routine case (EXP-105) — so when the client supplies the row's original
  // start scope, the row is re-created under the SAME id (fresh startedAt —
  // the original is lost with the row), restoring badge + steerability
  // within one heartbeat interval. An EXISTING non-running row is NEVER
  // resurrected: `ended` is an explicit end/kill and must stay final.
  // Fire-and-forget on the client: failures are reported, never thrown.
  heartbeat: authedProcedure
    .input(
      z
        .object({
          id: z.string().uuid(),
          // The row's original start scope — enables re-create-on-missing.
          issueId: z.string().uuid().optional(),
          workspaceId: z.string().uuid().optional(),
          deviceLabel: z.string().max(255).optional(),
        })
        .refine((value) => !(value.issueId && value.workspaceId), {
          message: `At most one of issueId/workspaceId`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      const [existing] = await ctx.db
        .select({
          userId: codingSessions.userId,
          status: codingSessions.status,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.id))
        .limit(1)

      if (!existing) {
        if (!input.issueId && !input.workspaceId) return { alive: false }
        try {
          if (input.issueId) {
            const issueCtx = await getIssueWorkspaceContext(input.issueId)
            await assertWorkspaceMember(ctx.session.user.id, issueCtx.workspaceId)
            await ctx.db.insert(codingSessions).values({
              id: input.id,
              issueId: input.issueId,
              workspaceId: issueCtx.workspaceId,
              projectId: issueCtx.projectId,
              userId: ctx.session.user.id,
              deviceLabel: input.deviceLabel ?? null,
              status: `running`,
            })
          } else {
            await assertWorkspaceMember(ctx.session.user.id, input.workspaceId!)
            await ctx.db.insert(codingSessions).values({
              id: input.id,
              workspaceId: input.workspaceId!,
              userId: ctx.session.user.id,
              deviceLabel: input.deviceLabel ?? null,
              status: `running`,
            })
          }
          return { alive: true }
        } catch {
          // Issue cascade-deleted, membership revoked, or an insert race —
          // degrade to the plain report; the next ping retries.
          return { alive: false }
        }
      }
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
