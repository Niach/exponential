import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { issues, projects, releases } from "@/db/schema"
import { dateOnlySchema } from "@/lib/domain"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"
import { recordIssueEvent } from "@/lib/integrations/activity"

// Releases (EXP-56): workspace-level issue bundles. Team-manageable — every
// mutation is gated on plain workspace MEMBERSHIP (no owner gate), matching
// the "manageable by the whole team" contract. Issue membership is 1:N via
// issues.release_id, so setIssueRelease/addIssues write the ISSUES table
// (clients await the issues collection txId); the other mutations write
// releases (await the releases collection txId). The release-PR fields
// (pr_url/pr_number/pr_state/pr_merged_at) are written only by the
// exponential_release_pr_open MCP tool + the GitHub webhook/poller — never
// from here.
export const releasesRouter = router({
  create: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255),
        description: z.string().max(60_000).optional(),
        targetDate: dateOnlySchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [release] = await tx
          .insert(releases)
          .values({
            workspaceId: input.workspaceId,
            name: input.name,
            description: input.description ?? null,
            targetDate: input.targetDate ?? null,
            createdBy: ctx.session.user.id,
          })
          .returning()
        return { txId, release }
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        description: z.string().max(60_000).nullable().optional(),
        targetDate: dateOnlySchema.nullable().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const release = await getReleaseOrThrow(ctx.db, input.id)
      await assertWorkspaceMember(ctx.session.user.id, release.workspaceId)

      const patch = {
        ...(input.name !== undefined ? { name: input.name } : {}),
        ...(input.description !== undefined
          ? { description: input.description }
          : {}),
        ...(input.targetDate !== undefined
          ? { targetDate: input.targetDate }
          : {}),
      }
      if (Object.keys(patch).length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Nothing to update`,
        })
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.update(releases).set(patch).where(eq(releases.id, input.id))
        return { txId }
      })
    }),

  // Manual ship/unship. The GitHub webhook also auto-ships when the linked
  // release PR merges (stamps shippedAt if still null) — this stays available
  // for repo-less releases and manual overrides.
  markShipped: authedProcedure
    .input(z.object({ id: z.string().uuid(), shipped: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const release = await getReleaseOrThrow(ctx.db, input.id)
      await assertWorkspaceMember(ctx.session.user.id, release.workspaceId)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(releases)
          .set({ shippedAt: input.shipped ? new Date() : null })
          .where(eq(releases.id, input.id))
        return { txId }
      })
    }),

  // Hard delete. issues.release_id FK is SET NULL (issues survive, unbundled);
  // release-scoped coding_sessions rows CASCADE away with their run history.
  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const release = await getReleaseOrThrow(ctx.db, input.id)
      await assertWorkspaceMember(ctx.session.user.id, release.workspaceId)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(releases).where(eq(releases.id, input.id))
        return { txId }
      })
    }),

  // Move ONE issue into a release (or out, with releaseId: null). Writes the
  // issues table — await the ISSUES collection txId client-side.
  setIssueRelease: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        releaseId: z.string().uuid().nullable(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueCtx = await getIssueWorkspaceContext(input.issueId)
      await assertWorkspaceMember(ctx.session.user.id, issueCtx.workspaceId)

      if (input.releaseId) {
        const release = await getReleaseOrThrow(ctx.db, input.releaseId)
        if (release.workspaceId !== issueCtx.workspaceId) {
          throw new TRPCError({
            code: `FORBIDDEN`,
            message: `Release and issue belong to different workspaces`,
          })
        }
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [previous] = await tx
          .select({ releaseId: issues.releaseId })
          .from(issues)
          .where(eq(issues.id, input.issueId))
          .limit(1)
        const previousReleaseId = previous?.releaseId ?? null

        await tx
          .update(issues)
          .set({ releaseId: input.releaseId })
          .where(eq(issues.id, input.issueId))

        if (previousReleaseId !== input.releaseId) {
          if (previousReleaseId) {
            await recordIssueEvent(tx, {
              issueId: input.issueId,
              workspaceId: issueCtx.workspaceId,
              actorUserId: ctx.session.user.id,
              type: `release_removed`,
              payload: { releaseId: previousReleaseId },
            })
          }
          if (input.releaseId) {
            await recordIssueEvent(tx, {
              issueId: input.issueId,
              workspaceId: issueCtx.workspaceId,
              actorUserId: ctx.session.user.id,
              type: `release_added`,
              payload: { releaseId: input.releaseId },
            })
          }
        }
        return { txId }
      })
    }),

  // Bulk-add from the release detail's "Add issues" picker. Silently skips
  // issues outside the release's workspace or in trashed projects (the picker
  // only offers valid ones; a stale selection shouldn't fail the whole batch).
  addIssues: authedProcedure
    .input(
      z.object({
        releaseId: z.string().uuid(),
        issueIds: z.array(z.string().uuid()).min(1).max(200),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const release = await getReleaseOrThrow(ctx.db, input.releaseId)
      await assertWorkspaceMember(ctx.session.user.id, release.workspaceId)

      const eligible = await ctx.db
        .select({ id: issues.id, releaseId: issues.releaseId })
        .from(issues)
        .innerJoin(projects, eq(issues.projectId, projects.id))
        .where(
          and(
            inArray(issues.id, input.issueIds),
            eq(projects.workspaceId, release.workspaceId),
            isNull(projects.deletedAt)
          )
        )
      // Issues already in this release are a no-op (no update, no event).
      const moving = eligible.filter((row) => row.releaseId !== input.releaseId)
      if (eligible.length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `No addable issues in this workspace`,
        })
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        if (moving.length > 0) {
          await tx
            .update(issues)
            .set({ releaseId: input.releaseId })
            .where(
              inArray(
                issues.id,
                moving.map((row) => row.id)
              )
            )
          for (const row of moving) {
            // An issue can be pulled over from another release — record both
            // sides so each timeline stays honest.
            if (row.releaseId) {
              await recordIssueEvent(tx, {
                issueId: row.id,
                workspaceId: release.workspaceId,
                actorUserId: ctx.session.user.id,
                type: `release_removed`,
                payload: { releaseId: row.releaseId },
              })
            }
            await recordIssueEvent(tx, {
              issueId: row.id,
              workspaceId: release.workspaceId,
              actorUserId: ctx.session.user.id,
              type: `release_added`,
              payload: { releaseId: input.releaseId },
            })
          }
        }
        return { txId, added: moving.length }
      })
    }),
})

type Db = typeof db

async function getReleaseOrThrow(db: Db, releaseId: string) {
  const [release] = await db
    .select()
    .from(releases)
    .where(eq(releases.id, releaseId))
    .limit(1)
  if (!release) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Release not found` })
  }
  return release
}
