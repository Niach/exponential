import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { issueLabels } from "@/db/schema"
import { and, eq } from "drizzle-orm"

export const issueLabelsRouter = router({
  add: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        labelId: z.string().uuid(),
      })
    )
    .mutation(async ({ input }) => {
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
    .mutation(async ({ input }) => {
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
