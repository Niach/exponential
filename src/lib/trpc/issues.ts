import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { issues, issueLabels, projects } from "@/db/schema"
import { eq } from "drizzle-orm"
import { assertWorkspaceMember } from "@/lib/workspace-membership"

const issueStatusValues = [
  `backlog`,
  `todo`,
  `in_progress`,
  `done`,
  `cancelled`,
] as const

const issuePriorityValues = [`none`, `urgent`, `high`, `medium`, `low`] as const

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
      // Look up project's workspace and check membership
      const [project] = await db
        .select({ workspaceId: projects.workspaceId })
        .from(projects)
        .where(eq(projects.id, input.projectId))
        .limit(1)
      if (project) {
        await assertWorkspaceMember(ctx.session.user.id, project.workspaceId)
      }

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
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input

      // Look up issue's project workspace and check membership
      const [existingIssue] = await db
        .select({ projectId: issues.projectId })
        .from(issues)
        .where(eq(issues.id, id))
        .limit(1)
      if (existingIssue) {
        const [project] = await db
          .select({ workspaceId: projects.workspaceId })
          .from(projects)
          .where(eq(projects.id, existingIssue.projectId))
          .limit(1)
        if (project) {
          await assertWorkspaceMember(ctx.session.user.id, project.workspaceId)
        }
      }

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
