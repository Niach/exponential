import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, issues, issueLabels, labels } from "@/db/schema"
import { eq, inArray } from "drizzle-orm"
import { workspaces } from "@/db/schema"
import {
  assertCanCreateIssueInProject,
  assertCanMutateIssue,
  isWorkspaceModerator,
} from "@/lib/workspace-membership"
import {
  dateOnlySchema,
  getIssueDescriptionText,
  issueDescriptionSchema,
  issuePrioritySchema,
  issueStatusSchema,
  recurrenceIntervalSchema,
  recurrenceUnitSchema,
  timeOnlySchema,
} from "@/lib/domain"
import {
  canonicalizeMarkdownImageUrls,
  extractAttachmentIdsFromDescription,
  hasMarkdownImages,
} from "@/lib/storage/issue-attachments"
import {
  collectAndDeleteRemovedAttachmentsInTx,
  collectAndDeleteUnreferencedAttachmentsInTx,
  collectIssueAttachmentStorageKeysInTx,
  deleteStorageObjects,
} from "@/lib/storage/issue-attachment-cleanup"
import { cloneIssueForRecurrence } from "@/lib/issue-recurrence"
import {
  fireAndForgetDelete,
  fireAndForgetSync,
} from "@/lib/integrations/google-calendar"
import { fireAndForgetAssignmentNotify } from "@/lib/integrations/notifications"

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
      const project = await assertCanCreateIssueInProject(
        ctx.session.user.id,
        input.projectId
      )

      // Non-moderators submitting to a public workspace can only set
      // title/description/labels — clamp the moderation fields so a stale or
      // tampered client can't bypass the UI restrictions.
      const [workspace] = await ctx.db
        .select({
          isPublic: workspaces.isPublic,
        })
        .from(workspaces)
        .where(eq(workspaces.id, project.workspaceId))
        .limit(1)
      const moderator = await isWorkspaceModerator(
        ctx.session.user.id,
        project.workspaceId
      )
      const restrictModeration = Boolean(workspace?.isPublic) && !moderator

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
            status: restrictModeration ? `backlog` : (input.status ?? `backlog`),
            priority: restrictModeration ? `none` : (input.priority ?? `none`),
            assigneeId: restrictModeration ? null : (input.assigneeId ?? null),
            description: input.description ?? null,
            dueDate: restrictModeration ? null : (input.dueDate ?? null),
            dueTime: restrictModeration ? null : (input.dueTime ?? null),
            endTime: restrictModeration ? null : (input.endTime ?? null),
            recurrenceInterval: restrictModeration
              ? null
              : (input.recurrenceInterval ?? null),
            recurrenceUnit: restrictModeration
              ? null
              : (input.recurrenceUnit ?? null),
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
      fireAndForgetAssignmentNotify({
        issueId: result.issue.id,
        actorUserId: ctx.session.user.id,
        newAssigneeId: result.issue.assigneeId,
      })

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
        archivedAt: z
          .union([z.string().datetime({ offset: true }), z.string().datetime()])
          .transform((s) => new Date(s))
          .nullable()
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input

      const issueContext = await assertCanMutateIssue(ctx.session.user.id, id)

      // Non-moderators (e.g., a non-member who created the issue in a public
      // workspace) may only touch title/description; strip moderation fields
      // before applying so a stale or tampered client can't bypass UI gating.
      const [workspace] = await ctx.db
        .select({ isPublic: workspaces.isPublic })
        .from(workspaces)
        .where(eq(workspaces.id, issueContext.workspaceId))
        .limit(1)
      const moderator = await isWorkspaceModerator(
        ctx.session.user.id,
        issueContext.workspaceId
      )
      if (workspace?.isPublic && !moderator) {
        delete (updates as Record<string, unknown>).status
        delete (updates as Record<string, unknown>).priority
        delete (updates as Record<string, unknown>).assigneeId
        delete (updates as Record<string, unknown>).dueDate
        delete (updates as Record<string, unknown>).dueTime
        delete (updates as Record<string, unknown>).endTime
        delete (updates as Record<string, unknown>).recurrenceInterval
        delete (updates as Record<string, unknown>).recurrenceUnit
        delete (updates as Record<string, unknown>).archivedAt
      }

      if (
        updates.recurrenceInterval !== undefined ||
        updates.recurrenceUnit !== undefined
      ) {
        assertRecurrencePair(updates.recurrenceInterval, updates.recurrenceUnit)
      }

      const deletedStorageKeys: string[] = []

      let previousAssigneeId: string | null = null
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

        previousAssigneeId = currentIssue.assigneeId
        const setValues: Record<string, unknown> = { ...updates }

        if (updates.status === `done` || updates.status === `cancelled`) {
          setValues.completedAt = new Date()
        } else if (updates.status) {
          setValues.completedAt = null
        }

        if (updates.description !== undefined) {
          const rawNextText = getIssueDescriptionText(updates.description)
          const previousText = getIssueDescriptionText(currentIssue.description)
          const { attachmentIds, invalidUrls } =
            extractAttachmentIdsFromDescription(rawNextText, ctx.request.url)

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

          // Canonicalize image URLs to the relative /api/attachments/{id} form
          // so stored markdown is client-agnostic, and persist the canonical
          // version (overriding the raw text the client submitted).
          const nextText =
            updates.description === null
              ? ``
              : canonicalizeMarkdownImageUrls(rawNextText, ctx.request.url)
          if (updates.description !== null) {
            setValues.description = { text: nextText }
          }

          const removedKeys = await collectAndDeleteRemovedAttachmentsInTx(
            tx,
            id,
            previousText,
            nextText,
            ctx.request.url
          )
          deletedStorageKeys.push(...removedKeys)

          // Reclaim never-referenced uploads left by an abandoned edit.
          const orphanKeys = await collectAndDeleteUnreferencedAttachmentsInTx(
            tx,
            id,
            nextText,
            ctx.request.url
          )
          deletedStorageKeys.push(...orphanKeys)
        }

        if (Object.keys(setValues).length === 0) {
          const [existing] = await tx
            .select()
            .from(issues)
            .where(eq(issues.id, id))
            .limit(1)
          return { issue: existing!, clonedIssue: null as typeof existing | null }
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
          const insertedClone = await cloneIssueForRecurrence(tx, {
            sourceIssueId: id,
            sourceProjectId: currentIssue.projectId,
            sourceTitle: currentIssue.title,
            sourcePriority: currentIssue.priority,
            sourceAssigneeId: currentIssue.assigneeId,
            sourceDescription: currentIssue.description,
            recurrenceInterval: nextRecurrenceInterval,
            recurrenceUnit: nextRecurrenceUnit,
            creatorId: ctx.session.user.id,
          })

          return { issue, clonedIssue: insertedClone }
        }

        return { issue, clonedIssue: null }
      })

      await deleteStorageObjects(deletedStorageKeys)

      fireAndForgetSync(ctx.session.user.id, issue)
      if (clonedIssue) {
        fireAndForgetSync(ctx.session.user.id, clonedIssue)
      }
      fireAndForgetAssignmentNotify({
        issueId: issue.id,
        actorUserId: ctx.session.user.id,
        newAssigneeId: issue.assigneeId,
        previousAssigneeId: previousAssigneeId,
      })

      return { issue }
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertCanMutateIssue(ctx.session.user.id, input.id)

      const storageKeys: Array<string> = []
      let googleCalendarEventId: string | null = null

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        storageKeys.push(
          ...(await collectIssueAttachmentStorageKeysInTx(tx, input.id))
        )

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

      await deleteStorageObjects(storageKeys)

      if (googleCalendarEventId) {
        fireAndForgetDelete(ctx.session.user.id, googleCalendarEventId)
      }

      return result
    }),
})
