import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, issues, issueLabels, labels } from "@/db/schema"
import { and, eq, inArray, sql } from "drizzle-orm"
import {
  assertProjectMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"
import {
  addRecurrence,
  dateOnlySchema,
  formatDateForMutation,
  getIssueDescriptionText,
  issueDescriptionSchema,
  issuePrioritySchema,
  issueStatusSchema,
  recurrenceIntervalSchema,
  recurrenceUnitSchema,
  timeOnlySchema,
} from "@/lib/domain"
import {
  extractAttachmentIdsFromDescription,
  getRemovedAttachmentIds,
  hasMarkdownImages,
  stripMarkdownImages,
} from "@/lib/issue-attachments"
import { deleteObject } from "@/lib/storage"
import {
  fireAndForgetDelete,
  fireAndForgetSync,
} from "@/lib/google-calendar"

function assertRecurrencePair(
  interval: number | null | undefined,
  unit: string | null | undefined
) {
  const intervalSet = interval !== null && interval !== undefined
  const unitSet = unit !== null && unit !== undefined

  if (intervalSet !== unitSet) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Recurrence interval and unit must be set together`,
    })
  }
}

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
        dueTime: timeOnlySchema.nullable().optional(),
        endTime: timeOnlySchema.nullable().optional(),
        labelIds: z.array(z.string().uuid()).optional(),
        recurrenceInterval: recurrenceIntervalSchema.nullable().optional(),
        recurrenceUnit: recurrenceUnitSchema.nullable().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const project = await assertProjectMember(
        ctx.session.user.id,
        input.projectId
      )

      assertRecurrencePair(input.recurrenceInterval, input.recurrenceUnit)

      if (input.description && hasMarkdownImages(input.description.text)) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Images can only be added after the issue is created`,
        })
      }

      const result = await ctx.db.transaction(async (tx) => {
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
            dueTime: input.dueTime ?? null,
            endTime: input.endTime ?? null,
            recurrenceInterval: input.recurrenceInterval ?? null,
            recurrenceUnit: input.recurrenceUnit ?? null,
            creatorId: ctx.session.user.id,
          })
          .returning()

        if (input.labelIds && input.labelIds.length > 0) {
          const labelRows = await tx
            .select({ id: labels.id, workspaceId: labels.workspaceId })
            .from(labels)
            .where(inArray(labels.id, input.labelIds))

          const wrongWorkspace = labelRows.find(
            (label) => label.workspaceId !== project.workspaceId
          )
          if (wrongWorkspace || labelRows.length !== input.labelIds.length) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Labels must belong to the same workspace as the project`,
            })
          }

          await tx.insert(issueLabels).values(
            input.labelIds.map((labelId) => ({
              issueId: issue.id,
              labelId,
              workspaceId: project.workspaceId,
            }))
          )
        }

        return { issue, txId }
      })

      if (result.issue.dueDate) {
        fireAndForgetSync(ctx.session.user.id, result.issue)
      }

      return result
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
        dueTime: timeOnlySchema.nullable().optional(),
        endTime: timeOnlySchema.nullable().optional(),
        recurrenceInterval: recurrenceIntervalSchema.nullable().optional(),
        recurrenceUnit: recurrenceUnitSchema.nullable().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input

      const issueContext = await getIssueWorkspaceContext(id)
      await assertProjectMember(ctx.session.user.id, issueContext.projectId)

      if (
        updates.recurrenceInterval !== undefined ||
        updates.recurrenceUnit !== undefined
      ) {
        assertRecurrencePair(updates.recurrenceInterval, updates.recurrenceUnit)
      }

      const deletedStorageKeys: string[] = []

      const { issue, clonedIssue } = await ctx.db.transaction(async (tx) => {
        const [currentIssue] = await tx
          .select({
            description: issues.description,
            status: issues.status,
            projectId: issues.projectId,
            title: issues.title,
            priority: issues.priority,
            assigneeId: issues.assigneeId,
            recurrenceInterval: issues.recurrenceInterval,
            recurrenceUnit: issues.recurrenceUnit,
          })
          .from(issues)
          .where(eq(issues.id, id))
          .limit(1)

        if (!currentIssue) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Issue not found`,
          })
        }

        const setValues: Record<string, unknown> = { ...updates }

        if (updates.status === `done` || updates.status === `cancelled`) {
          setValues.completedAt = new Date()
        } else if (updates.status) {
          setValues.completedAt = null
        }

        if (updates.description !== undefined) {
          const nextText = getIssueDescriptionText(updates.description)
          const previousText = getIssueDescriptionText(currentIssue.description)
          const { attachmentIds, invalidUrls } =
            extractAttachmentIdsFromDescription(nextText, ctx.request.url)

          if (invalidUrls.length > 0) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Issue descriptions can only reference uploaded issue images`,
            })
          }

          if (attachmentIds.length > 0) {
            const referencedAttachments = await tx
              .select({
                id: attachments.id,
                issueId: attachments.issueId,
              })
              .from(attachments)
              .where(inArray(attachments.id, attachmentIds))

            const allAttachmentsBelongToIssue =
              referencedAttachments.length === attachmentIds.length &&
              referencedAttachments.every(
                (attachment) => attachment.issueId === id
              )

            if (!allAttachmentsBelongToIssue) {
              throw new TRPCError({
                code: `BAD_REQUEST`,
                message: `Issue descriptions can only reference images uploaded to this issue`,
              })
            }
          }

          const removedAttachmentIds = getRemovedAttachmentIds(
            previousText,
            nextText,
            ctx.request.url
          )

          if (removedAttachmentIds.length > 0) {
            const removedAttachments = await tx
              .select({
                id: attachments.id,
                storageKey: attachments.storageKey,
              })
              .from(attachments)
              .where(
                and(
                  eq(attachments.issueId, id),
                  inArray(attachments.id, removedAttachmentIds)
                )
              )

            if (removedAttachments.length > 0) {
              deletedStorageKeys.push(
                ...removedAttachments.map((attachment) => attachment.storageKey)
              )

              await tx.delete(attachments).where(
                and(
                  eq(attachments.issueId, id),
                  inArray(
                    attachments.id,
                    removedAttachments.map((attachment) => attachment.id)
                  )
                )
              )
            }
          }
        }

        const [issue] = await tx
          .update(issues)
          .set(setValues)
          .where(eq(issues.id, id))
          .returning()

        const transitionedToDone =
          updates.status === `done` && currentIssue.status !== `done`
        const nextRecurrenceInterval =
          updates.recurrenceInterval !== undefined
            ? updates.recurrenceInterval
            : currentIssue.recurrenceInterval
        const nextRecurrenceUnit =
          updates.recurrenceUnit !== undefined
            ? updates.recurrenceUnit
            : currentIssue.recurrenceUnit

        if (
          transitionedToDone &&
          nextRecurrenceInterval !== null &&
          nextRecurrenceUnit !== null
        ) {
          const nextDueDate = formatDateForMutation(
            addRecurrence(
              new Date(),
              nextRecurrenceInterval,
              nextRecurrenceUnit
            )
          )

          const sourceDescriptionText = getIssueDescriptionText(
            currentIssue.description
          )
          const clonedDescription = sourceDescriptionText
            ? { text: stripMarkdownImages(sourceDescriptionText) }
            : null

          const [insertedClone] = await tx
            .insert(issues)
            .values({
              projectId: currentIssue.projectId,
              title: currentIssue.title,
              priority: currentIssue.priority,
              assigneeId: currentIssue.assigneeId,
              description: clonedDescription,
              status: `todo`,
              dueDate: nextDueDate,
              recurrenceInterval: nextRecurrenceInterval,
              recurrenceUnit: nextRecurrenceUnit,
              creatorId: ctx.session.user.id,
            })
            .returning()

          await tx.execute(sql`
            INSERT INTO ${issueLabels} (issue_id, label_id, workspace_id)
            SELECT ${insertedClone.id}::uuid, label_id, workspace_id
            FROM ${issueLabels}
            WHERE issue_id = ${id}::uuid
          `)

          return { issue, clonedIssue: insertedClone }
        }

        return { issue, clonedIssue: null }
      })

      if (deletedStorageKeys.length > 0) {
        await Promise.allSettled(
          deletedStorageKeys.map(async (storageKey) => {
            try {
              await deleteObject(storageKey)
            } catch (error) {
              console.error(`Failed to delete attachment object`, error)
            }
          })
        )
      }

      fireAndForgetSync(ctx.session.user.id, issue)
      if (clonedIssue) {
        fireAndForgetSync(ctx.session.user.id, clonedIssue)
      }

      return { issue }
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const issueContext = await getIssueWorkspaceContext(input.id)
      await assertProjectMember(ctx.session.user.id, issueContext.projectId)

      const storageKeys: Array<string> = []
      let googleCalendarEventId: string | null = null

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        const attachmentRows = await tx
          .select({ storageKey: attachments.storageKey })
          .from(attachments)
          .where(eq(attachments.issueId, input.id))
        storageKeys.push(...attachmentRows.map((row) => row.storageKey))

        const deleted = await tx
          .delete(issues)
          .where(eq(issues.id, input.id))
          .returning({
            id: issues.id,
            googleCalendarEventId: issues.googleCalendarEventId,
          })

        if (deleted.length === 0) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Issue not found`,
          })
        }

        googleCalendarEventId = deleted[0].googleCalendarEventId
        return { txId, id: deleted[0].id }
      })

      if (storageKeys.length > 0) {
        await Promise.allSettled(
          storageKeys.map(async (storageKey) => {
            try {
              await deleteObject(storageKey)
            } catch (error) {
              console.error(`Failed to delete attachment object`, error)
            }
          })
        )
      }

      if (googleCalendarEventId) {
        fireAndForgetDelete(ctx.session.user.id, googleCalendarEventId)
      }

      return result
    }),
})
