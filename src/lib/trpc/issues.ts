import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { issues, issueLabels } from "@/db/schema"
import { eq } from "drizzle-orm"

const issueStatusValues = [
  `backlog`,
  `todo`,
  `in_progress`,
  `done`,
  `cancelled`,
] as const

const issuePriorityValues = [
  `none`,
  `urgent`,
  `high`,
  `medium`,
  `low`,
] as const

export const issuesRouter = router({
  create: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        title: z.string().min(1).max(500),
        status: z.enum(issueStatusValues).optional(),
        priority: z.enum(issuePriorityValues).optional(),
        assigneeId: z.string().nullable().optional(),
        description: z.any().optional(),
        dueDate: z.string().nullable().optional(),
        labelIds: z.array(z.string().uuid()).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      return await db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [issue] = await tx
          .insert(issues)
          .values({
            projectId: input.projectId,
            title: input.title,
            status: input.status ?? `backlog`,
            priority: input.priority ?? `none`,
            assigneeId: input.assigneeId ?? null,
            description: input.description ?? null,
            dueDate: input.dueDate ?? null,
            creatorId: ctx.session.user.id,
          })
          .returning()

        if (input.labelIds && input.labelIds.length > 0) {
          await tx.insert(issueLabels).values(
            input.labelIds.map((labelId) => ({
              issueId: issue.id,
              labelId,
            }))
          )
        }

        return { issue, txId }
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        title: z.string().min(1).max(500).optional(),
        status: z.enum(issueStatusValues).optional(),
        priority: z.enum(issuePriorityValues).optional(),
        assigneeId: z.string().nullable().optional(),
        description: z.any().optional(),
        dueDate: z.string().nullable().optional(),
      })
    )
    .mutation(async ({ input }) => {
      const { id, ...updates } = input

      // Auto-manage completedAt based on status
      const setValues: Record<string, unknown> = { ...updates }
      if (updates.status === `done` || updates.status === `cancelled`) {
        setValues.completedAt = new Date()
      } else if (updates.status) {
        setValues.completedAt = null
      }

      const [issue] = await db
        .update(issues)
        .set(setValues)
        .where(eq(issues.id, id))
        .returning()

      return { issue }
    }),
})
