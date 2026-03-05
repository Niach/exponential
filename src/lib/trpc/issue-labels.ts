import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { issueLabels, labels } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { assertWorkspaceMember } from "@/lib/workspace-membership"

async function assertLabelMembership(
  userId: string,
  labelId: string
): Promise<void> {
  const [label] = await db
    .select({ workspaceId: labels.workspaceId })
    .from(labels)
    .where(eq(labels.id, labelId))
    .limit(1)
  if (label) {
    await assertWorkspaceMember(userId, label.workspaceId)
  }
}

export const issueLabelsRouter = router({
  add: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        labelId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertLabelMembership(ctx.session.user.id, input.labelId)
      return await db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .insert(issueLabels)
          .values({
            issueId: input.issueId,
            labelId: input.labelId,
          })
          .onConflictDoNothing()

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
      await assertLabelMembership(ctx.session.user.id, input.labelId)
      return await db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .delete(issueLabels)
          .where(
            and(
              eq(issueLabels.issueId, input.issueId),
              eq(issueLabels.labelId, input.labelId)
            )
          )

        return { txId }
      })
    }),
})
