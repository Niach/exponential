import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { issues, issueLabels } from "@/db/schema"
import { eq } from "drizzle-orm"
import {
  assertProjectMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"
import {
  dateOnlySchema,
  issueDescriptionSchema,
  issuePrioritySchema,
  issueStatusSchema,
} from "@/lib/domain"

export const issuesRouter = router({
  create: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        title: z.string().min(1).max(500),
        status: issueStatusSchema.optional(),
        priority: issuePrioritySchema.optional(),
        assigneeId: z.string().nullable().optional(),
        description: issueDescriptionSchema.optional(),
        dueDate: dateOnlySchema.nullable().optional(),
        labelIds: z.array(z.string().uuid()).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertProjectMember(ctx.session.user.id, input.projectId)

      return await ctx.db.transaction(async (tx) => {
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
        status: issueStatusSchema.optional(),
        priority: issuePrioritySchema.optional(),
        assigneeId: z.string().nullable().optional(),
        description: issueDescriptionSchema.nullable().optional(),
        dueDate: dateOnlySchema.nullable().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input

      const issueContext = await getIssueWorkspaceContext(id)
      await assertProjectMember(ctx.session.user.id, issueContext.projectId)

      const setValues: Record<string, unknown> = { ...updates }

      if (updates.status === `done` || updates.status === `cancelled`) {
        setValues.completedAt = new Date()
      } else if (updates.status) {
        setValues.completedAt = null
      }

      const [issue] = await ctx.db
        .update(issues)
        .set(setValues)
        .where(eq(issues.id, id))
        .returning()

      return { issue }
    }),
})
