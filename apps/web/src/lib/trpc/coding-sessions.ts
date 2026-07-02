import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { codingSessions } from "@/db/schema"
import { assertWithinCodingSessionLimit } from "@/lib/billing"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"

// The desktop launcher's live "coding now" record (§4a step 7). One row per
// interactive session; synced to every client as the 14th Electric shape.
// No generateTxId — native callers don't need the Electric tx-wait, and the
// row's own synced propagation carries the badge.
export const codingSessionsRouter = router({
  start: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        deviceLabel: z.string().max(255).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueCtx = await getIssueWorkspaceContext(input.issueId)
      await assertWorkspaceMember(ctx.session.user.id, issueCtx.workspaceId)
      // Plan capacity: concurrent running sessions per workspace — throws
      // PRECONDITION_FAILED with an upgrade nudge; self-hosted is unlimited.
      await assertWithinCodingSessionLimit(issueCtx.workspaceId)

      const [session] = await ctx.db
        .insert(codingSessions)
        .values({
          issueId: input.issueId,
          // Set explicitly (also trigger-denormalized) so the row is valid even
          // if the populate_coding_session_workspace_id trigger isn't applied.
          workspaceId: issueCtx.workspaceId,
          userId: ctx.session.user.id,
          deviceLabel: input.deviceLabel ?? null,
          status: `running`,
        })
        .returning()

      return { session }
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
