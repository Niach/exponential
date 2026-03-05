import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { labels } from "@/db/schema"
import { assertWorkspaceMember } from "@/lib/workspace-membership"

export const labelsRouter = router({
  create: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .default(`#6366f1`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId)
      return await db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [label] = await tx
          .insert(labels)
          .values({
            workspaceId: input.workspaceId,
            name: input.name,
            color: input.color,
          })
          .returning()

        return { txId, label }
      })
    }),
})
