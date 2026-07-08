import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { issueLabels } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { assertIssueLabelWorkspaceMatch } from "@/lib/workspace-membership"
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
      const { issue, label } = await assertIssueLabelWorkspaceMatch(
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
            workspaceId: label!.workspaceId,
            projectId: issue.projectId,
          })
          .onConflictDoNothing()

        await recordIssueEvent(tx, {
          issueId: input.issueId,
          workspaceId: label!.workspaceId,
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
      const { label } = await assertIssueLabelWorkspaceMatch(
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
          workspaceId: label!.workspaceId,
          actorUserId: ctx.session.user.id,
          type: `label_removed`,
          payload: { labelId: input.labelId },
        })

        return { txId }
      })
    }),
})
