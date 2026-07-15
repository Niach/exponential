import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  attachments,
  codingSessions,
  comments,
  issueEvents,
  issues,
  issueLabels,
  issueSubscribers,
  labels,
  projects,
  type Issue,
} from "@/db/schema"
import { and, eq, inArray, isNull, sql } from "drizzle-orm"
import {
  resolveWorkspaceAccess,
  assertAssigneeInWorkspace,
  assertIssueAccess,
  assertWorkspaceMember,
  getIssueWorkspaceContext,
  getProjectWorkspaceId,
  getSoleHumanMemberId,
} from "@/lib/workspace-membership"
import {
  closePullRequest,
  fetchPullFiles,
  GitHubMergeError,
  mergePullRequest,
} from "@/lib/integrations/github-pr"
import {
  githubAppConfigured,
  resolveRepoInstallationToken,
  resolveRepoInstallationTokenInfo,
} from "@/lib/integrations/github-app"
import { isInstallationLinkedToWorkspace } from "@/lib/trpc/integrations"
import { escapeLikePattern } from "@/lib/like-pattern"
import {
  applyPrClosedState,
  applyPrMergeState,
} from "@/lib/integrations/pr-sync"
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
  fireAndForgetIssueMentionNotify,
  fireAndForgetReporterResolution,
  fireAndForgetStatusChangeNotify,
} from "@/lib/integrations/notifications"
import { resolveMentions } from "@/lib/integrations/mentions"
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

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

// Status-derived column management, shared by update and bulkUpdate.
// Mutates setValues in place. Only applies the duplicate-clear rule when the
// caller hasn't already decided duplicate linkage (setValues.duplicateOfId
// === undefined) — update's duplicateOfId input block runs BEFORE this.
function applyStatusDerivations(
  setValues: Record<string, unknown>,
  current: { status: string; duplicateOfId: string | null }
): void {
  if (
    setValues.status !== undefined &&
    setValues.status !== `duplicate` &&
    current.duplicateOfId !== null &&
    setValues.duplicateOfId === undefined
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
    // Only an actual transition stamps completedAt — a redundant write of the
    // same terminal status must not clobber the original completion time.
    if (nextStatus !== current.status) {
      setValues.completedAt = new Date()
    }
  } else if (nextStatus) {
    setValues.completedAt = null
  }
}

// The per-issue write core shared by update and bulkUpdate: persists
// setValues, records status/assignee activity events (comparing the FINAL
// persisted values), auto-subscribes a new assignee, and clones the next
// occurrence of a recurring issue completed here. Post-commit side effects
// (attachment object copies, notification fan-out) are returned to the
// caller — never executed inside the transaction.
async function finalizeIssueUpdateInTx(
  tx: Tx,
  args: {
    issueId: string
    workspaceId: string
    actorUserId: string
    requestUrl: string
    current: {
      status: string
      projectId: string
      title: string
      priority: string
      assigneeId: string | null
      recurrenceInterval: number | null
      recurrenceUnit: string | null
    }
    setValues: Record<string, unknown>
  }
): Promise<{
  issue: typeof issues.$inferSelect
  statusChange: { from: string; to: string } | null
  previousAssigneeId: string | null
  attachmentCopies: AttachmentCopyOp[]
} | null> {
  const { issueId, workspaceId, actorUserId, requestUrl, current, setValues } =
    args

  const [issue] = await tx
    .update(issues)
    .set(setValues)
    .where(eq(issues.id, issueId))
    .returning()
  if (!issue) {
    // Hard-deleted between the caller's eligibility read and this UPDATE —
    // signal "row gone" instead of crashing the whole batch.
    return null
  }

  let statusChange: { from: string; to: string } | null = null
  if (current.status !== issue.status) {
    statusChange = { from: current.status, to: issue.status }
    await recordIssueEvent(tx, {
      issueId,
      workspaceId,
      actorUserId,
      type: `status_changed`,
      payload: { from: current.status, to: issue.status },
    })
  }
  if (current.assigneeId !== issue.assigneeId) {
    await recordIssueEvent(tx, {
      issueId,
      workspaceId,
      actorUserId,
      type: `assignee_changed`,
      payload: { from: current.assigneeId, to: issue.assigneeId },
    })
    if (issue.assigneeId) {
      await ensureSubscribed(tx, {
        issueId,
        userId: issue.assigneeId,
        workspaceId,
        source: `assignee`,
      })
    }
  }

  const attachmentCopies: AttachmentCopyOp[] = []
  const transitionedToDone =
    issue.status === `done` && current.status !== `done`
  const nextRecurrenceInterval =
    setValues.recurrenceInterval !== undefined
      ? (setValues.recurrenceInterval as number | null)
      : current.recurrenceInterval
  const nextRecurrenceUnit =
    setValues.recurrenceUnit !== undefined
      ? (setValues.recurrenceUnit as string | null)
      : current.recurrenceUnit

  if (
    transitionedToDone &&
    nextRecurrenceInterval !== null &&
    nextRecurrenceUnit !== null
  ) {
    const { attachmentCopies: copies } = await cloneIssueForRecurrence(tx, {
      sourceIssueId: issueId,
      sourceProjectId: current.projectId,
      sourceWorkspaceId: workspaceId,
      sourceTitle: current.title,
      sourcePriority: current.priority as Issue[`priority`],
      sourceAssigneeId: current.assigneeId,
      // Clone from the issue's final persisted description so it stays
      // consistent with any attachment cleanup that ran in this same
      // mutation (e.g. an image removed alongside completion).
      sourceDescription: issue.description,
      recurrenceInterval: nextRecurrenceInterval,
      recurrenceUnit: nextRecurrenceUnit as NonNullable<Issue[`recurrenceUnit`]>,
      creatorId: actorUserId,
      requestUrl,
    })
    attachmentCopies.push(...copies)
  }

  return {
    issue,
    statusChange,
    previousAssigneeId: current.assigneeId,
    attachmentCopies,
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

      // The assignee is INPUT, not the actor — validate it against the
      // issue's workspace or any member could push-notify arbitrary users.
      if (input.assigneeId != null) {
        await assertAssigneeInWorkspace(input.assigneeId, project.workspaceId)
      }

      // EXP-50: in a solo workspace (exactly one human member) an unassigned
      // issue can only ever be theirs — default-assign that member. An
      // explicit assignee (validated above) always wins; multi-member
      // workspaces keep the unassigned default.
      const assigneeId =
        input.assigneeId ?? (await getSoleHumanMemberId(project.workspaceId))

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
            status: input.status ?? `backlog`,
            priority: input.priority ?? `none`,
            assigneeId,
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
              projectId: input.projectId,
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

        // Description @mentions get the same treatment as comment mentions:
        // auto-subscribe here, issue_mention fan-out after commit.
        const mentionedUserIds = issue.description
          ? await resolveMentions(
              tx,
              getIssueDescriptionText(issue.description),
              project.workspaceId
            )
          : []
        for (const userId of mentionedUserIds) {
          await ensureSubscribed(tx, {
            issueId: issue.id,
            userId,
            workspaceId: project.workspaceId,
            source: `mention`,
          })
        }

        return { issue, txId, mentionedUserIds }
      })

      fireAndForgetAssignmentNotify({
        issueId: result.issue.id,
        actorUserId: ctx.session.user.id,
        newAssigneeId: result.issue.assigneeId,
      })
      // The assignee already gets issue_assigned — don't double-ping them
      // for also being mentioned (same "mention wins once" stance as the
      // comment fan-out, with assignment as the stronger signal here).
      const mentionNotifyIds = result.mentionedUserIds.filter(
        (userId) => userId !== result.issue.assigneeId
      )
      if (mentionNotifyIds.length > 0) {
        fireAndForgetIssueMentionNotify({
          issueId: result.issue.id,
          actorUserId: ctx.session.user.id,
          mentionedUserIds: mentionNotifyIds,
        })
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

      // The assignee is INPUT, not the actor — validate it against the
      // issue's workspace or any member could push-notify arbitrary users.
      // null (unassign) and undefined (untouched) both skip the check.
      if (updates.assigneeId != null) {
        await assertAssigneeInWorkspace(
          updates.assigneeId,
          issueContext.workspaceId
        )
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
      let newlyMentionedUserIds: string[] = []
      const { issue, statusChange } = await ctx.db.transaction(async (tx) => {
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
          // FOR UPDATE serializes concurrent updates of the same issue so the
          // transition checks below (recurrence spawn, status events) never
          // run against a stale snapshot — without it two concurrent 'done'
          // writes both see the old status and both spawn a recurrence clone.
          .for(`update`)

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
        }

        applyStatusDerivations(setValues, currentIssue)

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

          // Description @mentions, delta-based: only members mentioned in the
          // NEW text but not the old one are subscribed + notified, so
          // re-saving a description never re-pings everyone already in it.
          const previouslyMentioned = new Set(
            await resolveMentions(tx, previousText, issueContext.workspaceId)
          )
          const nextMentioned = await resolveMentions(
            tx,
            nextText,
            issueContext.workspaceId
          )
          newlyMentionedUserIds = nextMentioned.filter(
            (userId) => !previouslyMentioned.has(userId)
          )
          for (const userId of newlyMentionedUserIds) {
            await ensureSubscribed(tx, {
              issueId: id,
              userId,
              workspaceId: issueContext.workspaceId,
              source: `mention`,
            })
          }
        }

        if (Object.keys(setValues).length === 0) {
          const [existing] = await tx
            .select()
            .from(issues)
            .where(eq(issues.id, id))
            .limit(1)
          return {
            issue: existing!,
            statusChange: null as { from: string; to: string } | null,
          }
        }

        const result = await finalizeIssueUpdateInTx(tx, {
          issueId: id,
          workspaceId: issueContext.workspaceId,
          actorUserId: ctx.session.user.id,
          requestUrl: ctx.request.url,
          current: currentIssue,
          setValues,
        })
        if (!result) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }
        attachmentCopies.push(...result.attachmentCopies)

        return { issue: result.issue, statusChange: result.statusChange }
      })

      await deleteStorageObjects(deletedStorageKeys)
      await copyRecurrenceAttachments(attachmentCopies)

      fireAndForgetAssignmentNotify({
        issueId: issue.id,
        actorUserId: ctx.session.user.id,
        newAssigneeId: issue.assigneeId,
        previousAssigneeId: previousAssigneeId,
      })
      // A just-assigned user already gets issue_assigned — skip their
      // mention ping when both happen in the same update.
      const mentionNotifyIds =
        previousAssigneeId !== issue.assigneeId
          ? newlyMentionedUserIds.filter((userId) => userId !== issue.assigneeId)
          : newlyMentionedUserIds
      if (mentionNotifyIds.length > 0) {
        fireAndForgetIssueMentionNotify({
          issueId: issue.id,
          actorUserId: ctx.session.user.id,
          mentionedUserIds: mentionNotifyIds,
        })
      }
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

  // Move an issue to another project in the SAME workspace (EXP-57, web-only
  // UI for now). The issue is renumbered in the target project (Linear-style:
  // EXP-42 → ABC-17): the generate_issue_number trigger is INSERT-only, so the
  // next number is allocated here the same way the trigger does it (read the
  // target's current max, then upsert the monotonic issue_number_counters row
  // — the ON CONFLICT row lock serializes concurrent allocations and the
  // GREATEST clamp heals a stale/missing counter row). The denormalized child
  // project_id columns (their populate triggers are also INSERT-only) are
  // re-pointed in the same transaction so member + anonymous shape scoping
  // stays truthful. PR/branch linkage (pr_url/pr_number/branch) survives
  // untouched; labels are workspace-scoped, so they survive too.
  move: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        projectId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await assertIssueAccess(
        ctx.session.user.id,
        input.id,
        `write`
      )

      if (issueContext.projectId === input.projectId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issue is already in this project`,
        })
      }

      // 404s trashed targets — an issue must never move into the trash.
      const targetProject = await getProjectWorkspaceId(input.projectId)
      if (targetProject.workspaceId !== issueContext.workspaceId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issues can only move within their workspace`,
        })
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        // FOR UPDATE serializes concurrent moves of the same issue AND pairs
        // with the FOR KEY SHARE read in populate_issue_child_project_id so a
        // child row inserted mid-move can never commit with the old
        // project_id (see 0001_triggers.sql §7).
        const [current] = await tx
          .select({
            identifier: issues.identifier,
            projectId: issues.projectId,
          })
          .from(issues)
          .where(eq(issues.id, input.id))
          .limit(1)
          .for(`update`)
        if (!current) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }
        // Re-validate under the lock: a concurrent move that already landed
        // the issue here must not renumber it a second time (and would record
        // a project_moved event with a stale from-side).
        if (current.projectId === input.projectId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Issue is already in this project`,
          })
        }

        const [target] = await tx
          .select({
            prefix: projects.prefix,
            slug: projects.slug,
          })
          .from(projects)
          .where(eq(projects.id, input.projectId))
          .limit(1)
        if (!target) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Project not found`,
          })
        }

        // Allocate the target project's next number exactly like
        // generate_issue_number (0001_triggers.sql).
        const maxResult = await tx.execute(
          sql`SELECT COALESCE(MAX(number), 0) AS current_max FROM issues WHERE project_id = ${input.projectId}`
        )
        const currentMax = Number(
          (maxResult.rows[0] as { current_max: number | string }).current_max
        )
        const counterResult = await tx.execute(sql`
          INSERT INTO issue_number_counters AS c (project_id, counter)
          VALUES (${input.projectId}, ${currentMax} + 1)
          ON CONFLICT (project_id) DO UPDATE
            SET counter = GREATEST(c.counter, ${currentMax}) + 1
          RETURNING counter
        `)
        const nextNumber = Number(
          (counterResult.rows[0] as { counter: number | string }).counter
        )
        const nextIdentifier = `${target.prefix}-${nextNumber}`

        const [moved] = await tx
          .update(issues)
          .set({
            projectId: input.projectId,
            number: nextNumber,
            identifier: nextIdentifier,
          })
          .where(eq(issues.id, input.id))
          .returning()
        if (!moved) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }

        // Re-point the trigger-denormalized project_id on every issue-child
        // table (the populate triggers are INSERT-only). workspace_id is
        // unchanged — moves never cross workspaces.
        await tx
          .update(comments)
          .set({ projectId: input.projectId })
          .where(eq(comments.issueId, input.id))
        await tx
          .update(attachments)
          .set({ projectId: input.projectId })
          .where(eq(attachments.issueId, input.id))
        await tx
          .update(issueEvents)
          .set({ projectId: input.projectId })
          .where(eq(issueEvents.issueId, input.id))
        await tx
          .update(issueSubscribers)
          .set({ projectId: input.projectId })
          .where(eq(issueSubscribers.issueId, input.id))
        await tx
          .update(issueLabels)
          .set({ projectId: input.projectId })
          .where(eq(issueLabels.issueId, input.id))
        await tx
          .update(codingSessions)
          .set({ projectId: input.projectId })
          .where(eq(codingSessions.issueId, input.id))

        await recordIssueEvent(tx, {
          issueId: input.id,
          workspaceId: issueContext.workspaceId,
          actorUserId: ctx.session.user.id,
          type: `project_moved`,
          payload: {
            fromProjectId: current.projectId,
            toProjectId: input.projectId,
            fromIdentifier: current.identifier,
            toIdentifier: nextIdentifier,
          },
        })

        return { txId, issue: moved, projectSlug: target.slug }
      })
    }),

  // Bulk property write for the multi-select action bar (status / priority /
  // assignee). One workspace per batch, one transaction, one txId — Electric
  // awaitTxId covers every row version. Stale ids and issues in trashed
  // projects are silently skipped (addIssues precedent); an empty survivor
  // set is a hard error.
  bulkUpdate: authedProcedure
    .input(
      z
        .object({
          ids: z.array(z.string().uuid()).min(1).max(200),
          status: issueStatusSchema.optional(),
          priority: issuePrioritySchema.optional(),
          assigneeId: z.string().nullable().optional(),
        })
        .refine(
          (i) =>
            i.status !== undefined ||
            i.priority !== undefined ||
            i.assigneeId !== undefined,
          { message: `Nothing to update` }
        )
        // Bulk duplicate-marking has no canonical-issue picker, and
        // status='duplicate' with duplicateOfId=null breaks the pairing
        // invariant every single-issue path intercepts.
        .refine((i) => i.status !== `duplicate`, {
          message: `Duplicate requires a canonical issue — mark issues individually`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      const eligible = await ctx.db
        .select({
          id: issues.id,
          status: issues.status,
          projectId: issues.projectId,
          title: issues.title,
          priority: issues.priority,
          assigneeId: issues.assigneeId,
          recurrenceInterval: issues.recurrenceInterval,
          recurrenceUnit: issues.recurrenceUnit,
          duplicateOfId: issues.duplicateOfId,
          workspaceId: projects.workspaceId,
        })
        .from(issues)
        .innerJoin(projects, eq(issues.projectId, projects.id))
        .where(and(inArray(issues.id, input.ids), isNull(projects.deletedAt)))

      if (eligible.length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `No updatable issues`,
        })
      }
      const workspaceIds = new Set(eligible.map((row) => row.workspaceId))
      if (workspaceIds.size > 1) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issues must belong to one workspace`,
        })
      }
      const workspaceId = eligible[0].workspaceId
      await assertWorkspaceMember(ctx.session.user.id, workspaceId)

      // The assignee is INPUT, not the actor — validate it against the
      // batch's workspace or any member could push-notify arbitrary users.
      if (input.assigneeId != null) {
        await assertAssigneeInWorkspace(input.assigneeId, workspaceId)
      }

      const patch: Record<string, unknown> = {
        ...(input.status !== undefined ? { status: input.status } : {}),
        ...(input.priority !== undefined ? { priority: input.priority } : {}),
        ...(input.assigneeId !== undefined
          ? { assigneeId: input.assigneeId }
          : {}),
      }

      const { txId, results } = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const results: NonNullable<
          Awaited<ReturnType<typeof finalizeIssueUpdateInTx>>
        >[] = []
        for (const row of eligible) {
          const setValues: Record<string, unknown> = { ...patch }
          applyStatusDerivations(setValues, row)
          const result = await finalizeIssueUpdateInTx(tx, {
            issueId: row.id,
            workspaceId,
            actorUserId: ctx.session.user.id,
            requestUrl: ctx.request.url,
            current: row,
            setValues,
          })
          // Deleted in the window since the eligibility select — skip, keep
          // the batch (the eligibility filter promises silent-skip semantics).
          if (result) results.push(result)
        }
        return { txId, results }
      })

      await copyRecurrenceAttachments(
        results.flatMap((result) => result.attachmentCopies)
      )

      // Fan-out cap: a 200-issue sweep must not fire hundreds of pushes —
      // skip ALL per-issue notifications past 25 ids.
      if (input.ids.length <= 25) {
        for (const result of results) {
          if (result.previousAssigneeId !== result.issue.assigneeId) {
            fireAndForgetAssignmentNotify({
              issueId: result.issue.id,
              actorUserId: ctx.session.user.id,
              newAssigneeId: result.issue.assigneeId,
              previousAssigneeId: result.previousAssigneeId,
            })
          }
          if (result.statusChange) {
            fireAndForgetStatusChangeNotify({
              issueId: result.issue.id,
              actorUserId: ctx.session.user.id,
              fromStatus: result.statusChange.from,
              toStatus: result.statusChange.to,
            })
            fireAndForgetReporterResolution({
              issueId: result.issue.id,
              toStatus: result.statusChange.to,
            })
          }
        }
      }

      return { txId, updated: results.length }
    }),

  // Bulk delete for the multi-select action bar. Same gates as bulkUpdate
  // (write == delete == membership); attachment blobs are reclaimed from S3
  // after commit like the single delete.
  bulkDelete: authedProcedure
    .input(z.object({ ids: z.array(z.string().uuid()).min(1).max(200) }))
    .mutation(async ({ ctx, input }) => {
      const eligible = await ctx.db
        .select({ id: issues.id, workspaceId: projects.workspaceId })
        .from(issues)
        .innerJoin(projects, eq(issues.projectId, projects.id))
        .where(and(inArray(issues.id, input.ids), isNull(projects.deletedAt)))

      if (eligible.length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `No deletable issues`,
        })
      }
      const workspaceIds = new Set(eligible.map((row) => row.workspaceId))
      if (workspaceIds.size > 1) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issues must belong to one workspace`,
        })
      }
      await assertWorkspaceMember(ctx.session.user.id, eligible[0].workspaceId)

      const storageKeys: string[] = []
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        for (const row of eligible) {
          storageKeys.push(
            ...(await collectIssueAttachmentStorageKeysInTx(tx, row.id))
          )
        }
        const deleted = await tx
          .delete(issues)
          .where(
            inArray(
              issues.id,
              eligible.map((row) => row.id)
            )
          )
          .returning({ id: issues.id })
        return { txId, deleted: deleted.length }
      })

      await deleteStorageObjects(storageKeys)

      return result
    }),

  // Squash-merge the issue's open PR via the GitHub App installation token
  // (the symmetric counterpart of the MCP open_pr tool). Merging flips
  // prState/prMergedAt only — issue status stays a human decision. State
  // write + pr_merged event + notifications all go through the shared
  // applyPrMergeState writer, whose idempotent open→merged guard also absorbs
  // the later webhook delivery for the same merge.
  mergePr: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }): Promise<{ merged: true }> => {
      // Member-gated issue write (v7: every member is invited/trusted — the
      // old public-workspace moderator clamp is gone with self-service joins).
      await assertIssueAccess(ctx.session.user.id, input.issueId, `write`)

      const [row] = await ctx.db
        .select({
          prNumber: issues.prNumber,
          prUrl: issues.prUrl,
          prState: issues.prState,
          identifier: issues.identifier,
          title: issues.title,
        })
        .from(issues)
        .where(eq(issues.id, input.issueId))
        .limit(1)

      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
      }
      if (!row.prNumber || !row.prUrl) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This issue has no linked pull request`,
        })
      }
      if (row.prState === `merged`) {
        // Already merged (e.g. the webhook beat us) — idempotent no-op.
        return { merged: true }
      }
      if (row.prState !== `open`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The pull request is ${row.prState} — only open pull requests can be merged`,
        })
      }

      // Merge against the repo the PR actually lives in — derived from prUrl,
      // never the project's CURRENT repository: after a project repo
      // retarget, prNumber would otherwise address an unrelated PR in the
      // new repo (same derivation as prFiles below).
      const repoFullName = repoFromPrUrl(row.prUrl)
      if (!repoFullName) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The linked pull request URL is not a GitHub PR URL`,
        })
      }
      if (!githubAppConfigured()) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not configured on this instance`,
        })
      }
      const token = await resolveRepoInstallationToken(repoFullName)
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not installed on ${repoFullName}`,
        })
      }

      try {
        await mergePullRequest({
          repo: repoFullName,
          prNumber: row.prNumber,
          token,
          commitTitle: `${row.identifier}: ${row.title} (#${row.prNumber})`,
        })
      } catch (err) {
        if (err instanceof GitHubMergeError) {
          // 405 covers "not mergeable" and "squash merges not allowed" —
          // GitHub's message is the most useful thing to show verbatim.
          if (err.status === 405) {
            throw new TRPCError({
              code: `PRECONDITION_FAILED`,
              message: err.message,
            })
          }
          if (err.status === 409) {
            throw new TRPCError({
              code: `CONFLICT`,
              message: `Head branch changed on GitHub — refresh and try again`,
            })
          }
          if (err.status === 404) {
            throw new TRPCError({
              code: `NOT_FOUND`,
              message: `Pull request not found on GitHub`,
            })
          }
          throw new TRPCError({
            code: `INTERNAL_SERVER_ERROR`,
            message: `GitHub merge failed: ${err.message}`,
          })
        }
        throw err
      }

      await applyPrMergeState({
        issueId: input.issueId,
        prUrl: row.prUrl,
        mergedAt: new Date(),
        actorUserId: ctx.session.user.id,
      })

      return { merged: true }
    }),

  // Close the issue's open PR WITHOUT merging (EXP-100: the Reviews "reject"
  // path — the work exists on a branch but the issue got dropped). Mirrors
  // mergePr's guards/token resolution; the state flip goes through the shared
  // applyPrClosedState writer, whose open→closed guard also absorbs the later
  // webhook delivery for the same close. Issue status stays a human decision.
  closePr: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }): Promise<{ closed: true }> => {
      await assertIssueAccess(ctx.session.user.id, input.issueId, `write`)

      const [row] = await ctx.db
        .select({
          prNumber: issues.prNumber,
          prUrl: issues.prUrl,
          prState: issues.prState,
        })
        .from(issues)
        .where(eq(issues.id, input.issueId))
        .limit(1)

      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
      }
      if (!row.prNumber || !row.prUrl) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This issue has no linked pull request`,
        })
      }
      if (row.prState === `closed`) {
        // Already closed (e.g. the webhook beat us) — idempotent no-op.
        return { closed: true }
      }
      if (row.prState !== `open`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The pull request is ${row.prState} — only open pull requests can be closed`,
        })
      }

      // Close against the repo the PR actually lives in — derived from prUrl,
      // never the project's CURRENT repository (same derivation as mergePr).
      const repoFullName = repoFromPrUrl(row.prUrl)
      if (!repoFullName) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The linked pull request URL is not a GitHub PR URL`,
        })
      }
      if (!githubAppConfigured()) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not configured on this instance`,
        })
      }
      const token = await resolveRepoInstallationToken(repoFullName)
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not installed on ${repoFullName}`,
        })
      }

      try {
        await closePullRequest({
          repo: repoFullName,
          prNumber: row.prNumber,
          token,
        })
      } catch (err) {
        if (err instanceof GitHubMergeError) {
          if (err.status === 404) {
            throw new TRPCError({
              code: `NOT_FOUND`,
              message: `Pull request not found on GitHub`,
            })
          }
          throw new TRPCError({
            code: `INTERNAL_SERVER_ERROR`,
            message: `GitHub close failed: ${err.message}`,
          })
        }
        throw err
      }

      await applyPrClosedState({
        issueId: input.issueId,
        prUrl: row.prUrl,
      })

      return { closed: true }
    }),

  // Changed files for the issue's PR (one issue = one PR), for the diff view.
  // Fetched from GitHub server-side; see lib/integrations/github-pr.ts for the
  // token/visibility caveat.
  prFiles: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      // PR diffs can expose private-repo file contents — member-only, NEVER
      // public: anonymous feedback-board viewers have no authed session and
      // the issues shape hides pr_url/branch from them entirely.
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

      // Link-gate (mirrors repositories.installationToken): the installation
      // serving this repo must still be claimed by the issue's workspace — a
      // deliberately severed GitHub connection must not keep exposing
      // private-repo PR contents through an old prUrl.
      const resolved = await resolveRepoInstallationTokenInfo(repo)
      if (
        resolved &&
        !(await isInstallationLinkedToWorkspace(
          workspaceId,
          resolved.installationId
        ))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repo} resolves to a GitHub account that isn't connected to this workspace. Reconnect it in workspace settings → Repositories.`,
        })
      }

      try {
        const files = await fetchPullFiles(repo, row.prNumber, resolved?.token)
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

  // Full-text issue search (EXP-3): Postgres FTS over issue title +
  // description AND comment bodies, workspace-scoped, archived excluded,
  // relevance-ordered. An ILIKE substring fallback keeps this a strict
  // superset of the old title-substring search — it still matches
  // identifiers (EXP-42) and partial words that FTS lexemes miss. All
  // values are parameterized via drizzle `sql` interpolation. GIN
  // expression indexes on the tsvector expressions are a future scale
  // optimization (not needed at current volume).
  search: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        query: z.string().trim().min(1),
        limit: z.number().int().min(1).max(50).default(20),
      })
    )
    .query(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId)

      // Escape LIKE wildcards so the substring fallback matches literally.
      const like = `%${escapeLikePattern(input.query)}%`

      const result = await ctx.db.execute(sql`
        select
          i.id,
          i.identifier,
          i.title,
          i.project_id as "projectId",
          i.status,
          i.priority
        from issues i
        join projects p on p.id = i.project_id
        where p.workspace_id = ${input.workspaceId}::uuid
          and p.deleted_at is null
          and i.archived_at is null
          and (
            to_tsvector('english', coalesce(i.title, '') || ' ' || coalesce(i.description, ''))
              @@ websearch_to_tsquery('english', ${input.query})
            or exists (
              select 1 from comments c
              where c.issue_id = i.id
                and to_tsvector('english', c.body) @@ websearch_to_tsquery('english', ${input.query})
            )
            or i.title ilike ${like}
            or i.identifier ilike ${like}
            or i.description ilike ${like}
            or exists (
              select 1 from comments c2
              where c2.issue_id = i.id
                and c2.body ilike ${like}
            )
          )
        order by
          ts_rank(
            to_tsvector('english', coalesce(i.title, '') || ' ' || coalesce(i.description, '')),
            websearch_to_tsquery('english', ${input.query})
          ) desc,
          i.updated_at desc
        limit ${input.limit}
      `)

      return result.rows.map((row) => ({
        id: row.id as string,
        identifier: row.identifier as string,
        title: row.title as string,
        projectId: row.projectId as string,
        status: row.status as string,
        priority: row.priority as string,
      }))
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
