import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { actions, codingSessions, issues } from "@/db/schema"
import {
  assertTeamMember,
  getIssueTeamContext,
} from "@/lib/team-membership"

// The desktop launcher's live "coding now" record (§4a step 7). One row per
// interactive session; synced to every client as an Electric shape.
// Three subjects: issue-scoped (issueId), batch-scoped (teamId — the
// desktop multi-issue batch orchestrator; issue_id/board_id stay NULL, the
// populate triggers no-op on NULL issue_id), or action-scoped (actionId —
// EXP-253: batch-shaped plus action_id + the action_name display snapshot;
// actions are server-only so clients label rows off the snapshot).
// Exactly one of the three.
// No generateTxId — native callers don't need the Electric tx-wait, and the
// row's own synced propagation carries the badge.
export const codingSessionsRouter = router({
  start: authedProcedure
    .input(
      z
        .object({
          issueId: z.string().uuid().optional(),
          teamId: z.string().uuid().optional(),
          actionId: z.string().uuid().optional(),
          deviceLabel: z.string().max(255).optional(),
        })
        .refine(
          (value) =>
            [value.issueId, value.teamId, value.actionId].filter(Boolean)
              .length === 1,
          {
            message: `Exactly one of issueId/teamId/actionId is required`,
          }
        )
    )
    .mutation(async ({ ctx, input }) => {
      if (input.actionId) {
        // Action run: every member may run a team action (running is a
        // member affordance; only WRITES are owner-gated). The name is
        // snapshotted server-side so the row outlives the action row.
        const [action] = await ctx.db
          .select({
            id: actions.id,
            teamId: actions.teamId,
            name: actions.name,
          })
          .from(actions)
          .where(eq(actions.id, input.actionId))
          .limit(1)
        if (!action) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Action not found`,
          })
        }
        await assertTeamMember(ctx.session.user.id, action.teamId)

        const [session] = await ctx.db
          .insert(codingSessions)
          .values({
            // Batch-shaped: no issue/board — team_id written directly.
            teamId: action.teamId,
            actionId: action.id,
            actionName: action.name,
            userId: ctx.session.user.id,
            deviceLabel: input.deviceLabel ?? null,
            status: `running`,
          })
          .returning()

        return { session }
      }

      if (input.issueId) {
        const issueCtx = await getIssueTeamContext(input.issueId)
        await assertTeamMember(ctx.session.user.id, issueCtx.teamId)

        const [session] = await ctx.db
          .insert(codingSessions)
          .values({
            issueId: input.issueId,
            // Set explicitly (also trigger-denormalized) so the row is valid even
            // if the populate_* triggers aren't applied.
            teamId: issueCtx.teamId,
            boardId: issueCtx.boardId,
            userId: ctx.session.user.id,
            deviceLabel: input.deviceLabel ?? null,
            status: `running`,
          })
          .returning()

        return { session }
      }

      await assertTeamMember(ctx.session.user.id, input.teamId!)

      const [session] = await ctx.db
        .insert(codingSessions)
        .values({
          // Batch run: no issue to denormalize from — team_id written
          // directly; board_id stays NULL, a batch run spans boards and
          // must never surface through the anonymous board-scoped clause.
          teamId: input.teamId!,
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
  // within one heartbeat interval (an issue-scoped re-create derives
  // running/in_review from the issue's own status so a post-PR session
  // resurfaces with the right badge). An EXISTING `ended` row is NEVER
  // resurrected: `ended` is an explicit end/kill and must stay final.
  // `in_review` rows (PR open, terminal still alive — EXP-194) heartbeat
  // like running ones, but the ping only ever advances updated_at — it can
  // never downgrade in_review back to running.
  // Fire-and-forget on the client: failures are reported, never thrown.
  heartbeat: authedProcedure
    .input(
      z
        .object({
          id: z.string().uuid(),
          // The row's original start scope — enables re-create-on-missing.
          issueId: z.string().uuid().optional(),
          teamId: z.string().uuid().optional(),
          // Action scope (EXP-253) rides WITH teamId so a deleted action
          // still lets the row resurrect batch-shaped; actionName is the
          // client-held snapshot (the action may be gone by resurrect time).
          actionId: z.string().uuid().optional(),
          actionName: z.string().max(255).optional(),
          deviceLabel: z.string().max(255).optional(),
        })
        .refine((value) => !(value.issueId && value.teamId), {
          message: `At most one of issueId/teamId`,
        })
        .refine(
          (value) =>
            !value.actionId || (Boolean(value.teamId) && !value.issueId),
          { message: `actionId requires teamId and excludes issueId` }
        )
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
        if (!input.issueId && !input.teamId) return { alive: false }
        try {
          if (input.issueId) {
            const issueCtx = await getIssueTeamContext(input.issueId)
            await assertTeamMember(ctx.session.user.id, issueCtx.teamId)
            // A swept session may resurface AFTER its PR opened (laptop
            // suspend through the whole run) — re-derive the review badge
            // from the issue so the re-created row doesn't claim
            // "coding now" on a parked issue.
            const [issue] = await ctx.db
              .select({ status: issues.status })
              .from(issues)
              .where(eq(issues.id, input.issueId))
              .limit(1)
            await ctx.db.insert(codingSessions).values({
              id: input.id,
              issueId: input.issueId,
              teamId: issueCtx.teamId,
              boardId: issueCtx.boardId,
              userId: ctx.session.user.id,
              deviceLabel: input.deviceLabel ?? null,
              status: issue?.status === `in_review` ? `in_review` : `running`,
            })
          } else {
            await assertTeamMember(ctx.session.user.id, input.teamId!)
            // Action rows re-create from the client snapshot. If the action
            // was deleted meanwhile, a dangling-FK insert would 23503 —
            // pre-check and degrade: action_id NULL, actionName kept
            // (exactly the shape FK SET NULL leaves on live rows). The
            // pre-check races a concurrent delete; that lands in the catch
            // below and the next ping self-heals. The action must also
            // belong to the claimed team (the same derivation `start`
            // enforces) — a cross-team actionId degrades to NULL instead of
            // planting a cross-tenant FK reference in the synced row.
            let actionId: string | null = null
            if (input.actionId) {
              const [action] = await ctx.db
                .select({ id: actions.id, teamId: actions.teamId })
                .from(actions)
                .where(eq(actions.id, input.actionId))
                .limit(1)
              actionId =
                action && action.teamId === input.teamId ? action.id : null
            }
            await ctx.db.insert(codingSessions).values({
              id: input.id,
              teamId: input.teamId!,
              actionId,
              actionName: input.actionId ? (input.actionName ?? null) : null,
              userId: ctx.session.user.id,
              deviceLabel: input.deviceLabel ?? null,
              // Batch/action rows have no issue to re-derive review state
              // from — a resurrected session degrades to `running` (badge
              // label only; rare suspend edge, never kills anything).
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
      if (existing.status === `ended`) return { alive: false }

      // Status-conditioned so a heartbeat racing a kill/end can never
      // resurrect the row's freshness after it ended. The SET touches only
      // updatedAt — never status — so a ping cannot downgrade an
      // `in_review` row back to `running`.
      const updated = await ctx.db
        .update(codingSessions)
        .set({ updatedAt: new Date() })
        .where(
          and(
            eq(codingSessions.id, input.id),
            inArray(codingSessions.status, [`running`, `in_review`])
          )
        )
        .returning({ id: codingSessions.id })

      return { alive: updated.length > 0 }
    }),

  // Desktop-only attention flag (EXP-214): flips `needs_input` when the
  // agent parks on a plan-approval / AskUserQuestion picker (the steer
  // activity emitter's picker watchers) and clears it when the picker
  // resolves. Deliberately a separate boolean, not a status — running/
  // in_review stay server-owned and a ping can never race the PR-open flip.
  // Fire-and-forget on the client like heartbeat: failures are never thrown
  // into the terminal path.
  setNeedsInput: authedProcedure
    .input(z.object({ id: z.string().uuid(), needsInput: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const [existing] = await ctx.db
        .select({
          userId: codingSessions.userId,
          status: codingSessions.status,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.id))
        .limit(1)

      if (!existing) return { updated: false }
      if (existing.userId !== ctx.session.user.id) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the session owner can update it`,
        })
      }

      // Status-conditioned like heartbeat: an ended row stays final and
      // never re-surfaces as "needs input".
      const updated = await ctx.db
        .update(codingSessions)
        .set({ needsInput: input.needsInput })
        .where(
          and(
            eq(codingSessions.id, input.id),
            inArray(codingSessions.status, [`running`, `in_review`])
          )
        )
        .returning({ id: codingSessions.id })

      return { updated: updated.length > 0 }
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
