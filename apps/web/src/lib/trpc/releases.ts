import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, sql } from "drizzle-orm"
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
// releases (await the releases collection txId). `create` with issueIds
// (EXP-62) writes BOTH in one transaction — the returned txId covers the
// releases AND issues collections. The release-PR fields
// (pr_url/pr_number/pr_state/pr_merged_at) are written only by the
// exponential_release_pr_open MCP tool + the GitHub webhook/poller — never
// from here.
export const releasesRouter = router({
  // Create: name is optional — when absent the server auto-names
  // sequentially ("Release N", N = max existing trailing integer + 1).
  // Description/targetDate are set post-create via `update` (inline editing).
  // issueIds is the creation-time bundle (EXP-62): clients pick the issues
  // BEFORE the release exists; the attach happens in the SAME transaction as
  // the insert (addIssues semantics — timeline events included). It stays
  // optional so older native clients calling plain create keep working.
  create: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        issueIds: z.array(z.string().uuid()).min(1).max(200).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId)

      // Pre-tx eligibility read (the addIssues pattern): silently drops
      // workspace-foreign / trashed-project ids, but refuses when the
      // requested bundle leaves nothing — an empty release is exactly what
      // creation-time picking exists to prevent.
      let attach: AttachableIssue[] = []
      if (input.issueIds) {
        attach = await selectAttachableIssues(
          ctx.db,
          input.workspaceId,
          input.issueIds
        )
        if (attach.length === 0) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `No addable issues in this workspace`,
          })
        }
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        let name = input.name
        if (!name) {
          // Serialize concurrent auto-names per workspace: without this lock
          // two simultaneous creates both read the same max and insert a
          // duplicate "Release N" (name has no unique constraint by design).
          await tx.execute(
            sql`SELECT pg_advisory_xact_lock(hashtext(${input.workspaceId}))`
          )
          const rows = await tx
            .select({ name: releases.name })
            .from(releases)
            .where(eq(releases.workspaceId, input.workspaceId))
          const max = rows.reduce((acc, row) => {
            const match = /^Release (\d+)$/.exec(row.name)
            return match ? Math.max(acc, Number(match[1])) : acc
          }, 0)
          name = `Release ${max + 1}`
        }
        const [release] = await tx
          .insert(releases)
          .values({
            workspaceId: input.workspaceId,
            name,
            createdBy: ctx.session.user.id,
          })
          .returning()
        // A brand-new release can't already hold any of the picked issues,
        // so every eligible row moves (incl. pulls from other releases —
        // both timeline sides recorded, like addIssues).
        await attachIssuesInTx(tx, {
          releaseId: release.id,
          workspaceId: input.workspaceId,
          actorUserId: ctx.session.user.id,
          moving: attach,
        })
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

      const eligible = await selectAttachableIssues(
        ctx.db,
        release.workspaceId,
        input.issueIds
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
        await attachIssuesInTx(tx, {
          releaseId: input.releaseId,
          workspaceId: release.workspaceId,
          actorUserId: ctx.session.user.id,
          moving,
        })
        return { txId, added: moving.length }
      })
    }),
})

type Db = typeof db
type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]

type AttachableIssue = { id: string; releaseId: string | null }

// The shared attach eligibility read (create + addIssues): issues from the
// batch that live in the target workspace, in non-trashed projects. Runs
// pre-transaction — a stale/foreign id silently drops out instead of failing
// the whole batch.
async function selectAttachableIssues(
  database: Db,
  workspaceId: string,
  issueIds: string[]
): Promise<AttachableIssue[]> {
  return await database
    .select({ id: issues.id, releaseId: issues.releaseId })
    .from(issues)
    .innerJoin(projects, eq(issues.projectId, projects.id))
    .where(
      and(
        inArray(issues.id, issueIds),
        eq(projects.workspaceId, workspaceId),
        isNull(projects.deletedAt)
      )
    )
}

// The shared in-tx attach (create + addIssues): bulk-move `moving` into the
// release and record the timeline events. An issue can be pulled over from
// another release — record both sides so each timeline stays honest.
async function attachIssuesInTx(
  tx: Tx,
  args: {
    releaseId: string
    workspaceId: string
    actorUserId: string
    moving: AttachableIssue[]
  }
): Promise<void> {
  if (args.moving.length === 0) return
  await tx
    .update(issues)
    .set({ releaseId: args.releaseId })
    .where(
      inArray(
        issues.id,
        args.moving.map((row) => row.id)
      )
    )
  for (const row of args.moving) {
    if (row.releaseId) {
      await recordIssueEvent(tx, {
        issueId: row.id,
        workspaceId: args.workspaceId,
        actorUserId: args.actorUserId,
        type: `release_removed`,
        payload: { releaseId: row.releaseId },
      })
    }
    await recordIssueEvent(tx, {
      issueId: row.id,
      workspaceId: args.workspaceId,
      actorUserId: args.actorUserId,
      type: `release_added`,
      payload: { releaseId: args.releaseId },
    })
  }
}

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
