import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, issues, issueLabels, labels, projects } from "@/db/schema"
import { eq, inArray } from "drizzle-orm"
import {
  resolveWorkspaceAccess,
  assertIssueAccess,
  assertWorkspaceMember,
  getIssueWorkspaceContext,
  getProjectWorkspaceId,
  isModerationRestricted,
  applyModerationRestrictions,
} from "@/lib/workspace-membership"
import {
  fetchPullFiles,
  resolveRepoToken,
} from "@/lib/integrations/github-pr"
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
import {
  cloneIssueForRecurrence,
  copyRecurrenceAttachments,
  type AttachmentCopyOp,
} from "@/lib/issue-recurrence"
import {
  fireAndForgetAssignmentNotify,
  fireAndForgetReporterResolution,
  fireAndForgetStatusChangeNotify,
} from "@/lib/integrations/notifications"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import { recordIssueEvent } from "@/lib/integrations/activity"

// Extract `owner/repo` from a GitHub PR URL
// (https://github.com/owner/repo/pull/123). Returns null if it doesn't match.
function repoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(
    /github\.com\/([^/]+\/[^/]+)\/pull\/\d+/
  )
  return match ? match[1] : null
}

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
      const project = await getProjectWorkspaceId(input.projectId)
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        project.workspaceId,
        `create_issue`
      )

      // Non-moderators submitting to a public workspace can only set
      // title/description/labels — clamp the moderation fields so a stale or
      // tampered client can't bypass the UI restrictions.
      const restrictModeration = await isModerationRestricted(
        ctx.session.user.id,
        project.workspaceId
      )

      assertRecurrencePair(input.recurrenceInterval, input.recurrenceUnit)

      if (input.description && hasMarkdownImages(input.description)) {
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

        // Auto-subscribe the creator (and assignee, if any) so they get inbox
        // activity. Agents are skipped inside ensureSubscribed.
        await ensureSubscribed(tx, {
          issueId: issue.id,
          userId: ctx.session.user.id,
          workspaceId: project.workspaceId,
          source: `creator`,
        })
        if (issue.assigneeId) {
          await ensureSubscribed(tx, {
            issueId: issue.id,
            userId: issue.assigneeId,
            workspaceId: project.workspaceId,
            source: `assignee`,
          })
        }

        return { issue, txId }
      })

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
        // Canonical issue this one duplicates. Kept in lockstep with the
        // 'duplicate' status inside the transaction below: marking forces
        // status='duplicate'; unmarking (null) restores backlog; moving to any
        // other status clears the link.
        duplicateOfId: z.string().uuid().nullable().optional(),
        archivedAt: z
          .union([z.string().datetime({ offset: true }), z.string().datetime()])
          .transform((s) => new Date(s))
          .nullable()
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input

      const issueContext = await assertIssueAccess(
        ctx.session.user.id,
        id,
        `write`
      )

      // Non-moderators (e.g., a non-member who created the issue in a public
      // workspace) may only touch title/description; strip moderation fields
      // before applying so a stale or tampered client can't bypass UI gating.
      if (
        await isModerationRestricted(
          ctx.session.user.id,
          issueContext.workspaceId
        )
      ) {
        applyModerationRestrictions(updates as Record<string, unknown>)
        // duplicateOfId isn't in the shared moderation field list (it's
        // web-schema-specific), but it drives a status change — strip it too.
        delete updates.duplicateOfId
      }

      if (
        updates.recurrenceInterval !== undefined ||
        updates.recurrenceUnit !== undefined
      ) {
        assertRecurrencePair(updates.recurrenceInterval, updates.recurrenceUnit)
      }

      const deletedStorageKeys: string[] = []
      const attachmentCopies: AttachmentCopyOp[] = []

      let previousAssigneeId: string | null = null
      const { issue, statusChange } = await ctx.db.transaction(async (tx) => {
        let statusChange: { from: string; to: string } | null = null
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
            duplicateOfId: issues.duplicateOfId,
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

        // Keep duplicateOfId and the 'duplicate' status in lockstep,
        // atomically in this one UPDATE (masterplan §5e).
        if (updates.duplicateOfId !== undefined) {
          if (updates.duplicateOfId !== null) {
            if (updates.duplicateOfId === id) {
              throw new TRPCError({
                code: `BAD_REQUEST`,
                message: `An issue cannot be a duplicate of itself`,
              })
            }
            // The canonical issue must live in the same workspace.
            const [canonical] = await tx
              .select({ workspaceId: projects.workspaceId })
              .from(issues)
              .innerJoin(projects, eq(projects.id, issues.projectId))
              .where(eq(issues.id, updates.duplicateOfId))
              .limit(1)
            if (
              !canonical ||
              canonical.workspaceId !== issueContext.workspaceId
            ) {
              throw new TRPCError({
                code: `BAD_REQUEST`,
                message: `Canonical issue must be in the same workspace`,
              })
            }
            setValues.status = `duplicate`
          } else if ((updates.status ?? currentIssue.status) === `duplicate`) {
            // Unmarking: 'duplicate' no longer applies. The prior status isn't
            // stored, so restore the neutral default.
            setValues.status = `backlog`
          }
        } else if (
          updates.status !== undefined &&
          updates.status !== `duplicate` &&
          currentIssue.duplicateOfId !== null
        ) {
          // Moving off 'duplicate' via a plain status change also unmarks.
          setValues.duplicateOfId = null
        }

        const nextStatus = setValues.status as string | undefined
        if (
          nextStatus === `done` ||
          nextStatus === `cancelled` ||
          nextStatus === `duplicate`
        ) {
          setValues.completedAt = new Date()
        } else if (nextStatus) {
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
            setValues.description = nextText
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
          return {
            issue: existing!,
            clonedIssue: null as typeof existing | null,
            statusChange,
          }
        }

        const [issue] = await tx
          .update(issues)
          .set(setValues)
          .where(eq(issues.id, id))
          .returning()

        // Activity-log events for status / assignee changes (compare the final
        // persisted values, so moderation-stripped updates don't emit events).
        if (currentIssue.status !== issue.status) {
          statusChange = { from: currentIssue.status, to: issue.status }
          await recordIssueEvent(tx, {
            issueId: id,
            workspaceId: issueContext.workspaceId,
            actorUserId: ctx.session.user.id,
            type: `status_changed`,
            payload: { from: currentIssue.status, to: issue.status },
          })
        }
        if (previousAssigneeId !== issue.assigneeId) {
          await recordIssueEvent(tx, {
            issueId: id,
            workspaceId: issueContext.workspaceId,
            actorUserId: ctx.session.user.id,
            type: `assignee_changed`,
            payload: { from: previousAssigneeId, to: issue.assigneeId },
          })
          if (issue.assigneeId) {
            await ensureSubscribed(tx, {
              issueId: id,
              userId: issue.assigneeId,
              workspaceId: issueContext.workspaceId,
              source: `assignee`,
            })
          }
        }

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
          const { issue: insertedClone, attachmentCopies: copies } =
            await cloneIssueForRecurrence(tx, {
              sourceIssueId: id,
              sourceProjectId: currentIssue.projectId,
              sourceWorkspaceId: issueContext.workspaceId,
              sourceTitle: currentIssue.title,
              sourcePriority: currentIssue.priority,
              sourceAssigneeId: currentIssue.assigneeId,
              // Clone from the issue's final persisted description so it stays
              // consistent with any attachment cleanup that ran in this same
              // mutation (e.g. an image removed alongside completion).
              sourceDescription: issue.description,
              recurrenceInterval: nextRecurrenceInterval,
              recurrenceUnit: nextRecurrenceUnit,
              creatorId: ctx.session.user.id,
              requestUrl: ctx.request.url,
            })
          attachmentCopies.push(...copies)

          return { issue, clonedIssue: insertedClone, statusChange }
        }

        return { issue, clonedIssue: null, statusChange }
      })

      await deleteStorageObjects(deletedStorageKeys)
      await copyRecurrenceAttachments(attachmentCopies)

      fireAndForgetAssignmentNotify({
        issueId: issue.id,
        actorUserId: ctx.session.user.id,
        newAssigneeId: issue.assigneeId,
        previousAssigneeId: previousAssigneeId,
      })
      if (statusChange) {
        fireAndForgetStatusChangeNotify({
          issueId: issue.id,
          actorUserId: ctx.session.user.id,
          fromStatus: statusChange.from,
          toStatus: statusChange.to,
        })
        // One-way helpdesk: closing a widget-reported issue emails the
        // external reporter once (idempotent via resolvedNotifiedAt).
        fireAndForgetReporterResolution({
          issueId: issue.id,
          toStatus: statusChange.to,
        })
      }

      return { issue }
    }),

  // Changed files for the issue's PR (one issue = one PR), for the diff view.
  // Fetched from GitHub server-side; see lib/integrations/github-pr.ts for the
  // token/visibility caveat.
  prFiles: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const { workspaceId } = await getIssueWorkspaceContext(input.issueId)
      await assertWorkspaceMember(ctx.session.user.id, workspaceId)

      const [row] = await ctx.db
        .select({
          prNumber: issues.prNumber,
          prUrl: issues.prUrl,
        })
        .from(issues)
        .where(eq(issues.id, input.issueId))
        .limit(1)

      // Derive owner/repo from the PR URL (repos no longer live on projects —
      // they moved to the server-only repositories registry).
      const repo = row?.prUrl ? repoFromPrUrl(row.prUrl) : null
      if (!row?.prNumber || !repo) {
        return { repo: null as string | null, prNumber: null, files: [] }
      }

      try {
        const token = await resolveRepoToken({
          actorUserId: ctx.session.user.id,
          workspaceId,
          repo,
        })
        const files = await fetchPullFiles(repo, row.prNumber, token)
        return { repo, prNumber: row.prNumber, files }
      } catch (err) {
        throw new TRPCError({
          code: `BAD_GATEWAY`,
          message:
            err instanceof Error
              ? err.message
              : `Failed to load changes from GitHub`,
        })
      }
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertIssueAccess(ctx.session.user.id, input.id, `delete`)

      const storageKeys: Array<string> = []

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        storageKeys.push(
          ...(await collectIssueAttachmentStorageKeysInTx(tx, input.id))
        )

        const deleted = await tx
          .delete(issues)
          .where(eq(issues.id, input.id))
          .returning({ id: issues.id })

        if (deleted.length === 0) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Issue not found`,
          })
        }

        return { txId, id: deleted[0].id }
      })

      await deleteStorageObjects(storageKeys)

      return result
    }),
})
