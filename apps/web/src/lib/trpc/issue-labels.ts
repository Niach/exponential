import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { issueLabels, issues, labels, boards } from "@/db/schema"
import { and, eq, inArray, isNull } from "drizzle-orm"
import {
  assertIssueLabelTeamMatch,
  assertTeamMember,
} from "@/lib/team-membership"
import { recordIssueEvent } from "@/lib/integrations/activity"

export const issueLabelsRouter = router({
  add: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        labelId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { issue, label } = await assertIssueLabelTeamMatch(
        ctx.session.user.id,
        input.issueId,
        input.labelId
      )

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .insert(issueLabels)
          .values({
            issueId: input.issueId,
            labelId: input.labelId,
            teamId: label!.teamId,
            boardId: issue.boardId,
          })
          .onConflictDoNothing()

        await recordIssueEvent(tx, {
          issueId: input.issueId,
          teamId: label!.teamId,
          actorUserId: ctx.session.user.id,
          type: `label_added`,
          payload: { labelId: input.labelId },
        })

        return { txId }
      })
    }),

  remove: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        labelId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { label } = await assertIssueLabelTeamMatch(
        ctx.session.user.id,
        input.issueId,
        input.labelId
      )

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .delete(issueLabels)
          .where(
            and(
              eq(issueLabels.issueId, input.issueId),
              eq(issueLabels.labelId, input.labelId)
            )
          )

        await recordIssueEvent(tx, {
          issueId: input.issueId,
          teamId: label!.teamId,
          actorUserId: ctx.session.user.id,
          type: `label_removed`,
          payload: { labelId: input.labelId },
        })

        return { txId }
      })
    }),

  // Bulk label writes for the multi-select action bar: ONE label across many
  // issues. Eligibility mirrors issues.bulkUpdate — same team as the
  // label, non-trashed board; stale ids are silently skipped. Events are
  // recorded only for rows actually inserted/deleted (returning()), so a
  // half-labelled selection never double-logs the already-labelled issues.
  bulkAdd: authedProcedure
    .input(
      z.object({
        labelId: z.string().uuid(),
        issueIds: z.array(z.string().uuid()).min(1).max(200),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const label = await getLabelOrThrow(ctx.db, input.labelId)
      await assertTeamMember(ctx.session.user.id, label.teamId)

      return await ctx.db.transaction(async (tx) => {
        // Eligibility read runs IN the insert's transaction — read outside,
        // an issue hard-deleted in the window would FK-fail the whole batch
        // instead of being silently skipped.
        const eligible = await tx
          .select({ id: issues.id, boardId: issues.boardId })
          .from(issues)
          .innerJoin(boards, eq(issues.boardId, boards.id))
          .where(
            and(
              inArray(issues.id, input.issueIds),
              eq(boards.teamId, label.teamId),
              isNull(boards.deletedAt)
            )
          )
        if (eligible.length === 0) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `No labelable issues in this team`,
          })
        }

        const txId = await generateTxId(tx)
        const inserted = await tx
          .insert(issueLabels)
          .values(
            eligible.map((row) => ({
              issueId: row.id,
              labelId: input.labelId,
              teamId: label.teamId,
              boardId: row.boardId,
            }))
          )
          .onConflictDoNothing()
          .returning({ issueId: issueLabels.issueId })

        for (const row of inserted) {
          await recordIssueEvent(tx, {
            issueId: row.issueId,
            teamId: label.teamId,
            actorUserId: ctx.session.user.id,
            type: `label_added`,
            payload: { labelId: input.labelId },
          })
        }

        return { txId }
      })
    }),

  bulkRemove: authedProcedure
    .input(
      z.object({
        labelId: z.string().uuid(),
        issueIds: z.array(z.string().uuid()).min(1).max(200),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const label = await getLabelOrThrow(ctx.db, input.labelId)
      await assertTeamMember(ctx.session.user.id, label.teamId)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const removed = await tx
          .delete(issueLabels)
          .where(
            and(
              eq(issueLabels.labelId, input.labelId),
              inArray(issueLabels.issueId, input.issueIds)
            )
          )
          .returning({ issueId: issueLabels.issueId })

        for (const row of removed) {
          await recordIssueEvent(tx, {
            issueId: row.issueId,
            teamId: label.teamId,
            actorUserId: ctx.session.user.id,
            type: `label_removed`,
            payload: { labelId: input.labelId },
          })
        }

        return { txId }
      })
    }),
})

type Db = typeof db

async function getLabelOrThrow(db: Db, labelId: string) {
  const [label] = await db
    .select({ teamId: labels.teamId })
    .from(labels)
    .where(eq(labels.id, labelId))
    .limit(1)
  if (!label) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Label not found` })
  }
  return label
}
