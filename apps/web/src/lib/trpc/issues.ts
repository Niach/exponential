import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { attachments, issues, issueLabels, labels, projects } from "@/db/schema"
import { eq, inArray, sql } from "drizzle-orm"
import {
  resolveWorkspaceAccess,
  assertAssigneeInWorkspace,
  assertIssueAccess,
  assertWorkspaceMember,
  getIssueWorkspaceContext,
  getProjectWorkspaceId,
} from "@/lib/workspace-membership"
import {
  fetchPullFiles,
  GitHubMergeError,
  mergePullRequest,
  resolveRepoToken,
} from "@/lib/integrations/github-pr"
import {
  githubAppConfigured,
  resolveRepoInstallationToken,
} from "@/lib/integrations/github-app"
import { applyPrMergeState } from "@/lib/integrations/pr-sync"
import { resolveProjectRepository } from "@/lib/trpc/repositories"
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

      // The assignee is INPUT, not the actor — validate it against the
      // issue's workspace or any member could push-notify arbitrary users.
      if (input.assigneeId != null) {
        await assertAssigneeInWorkspace(input.assigneeId, project.workspaceId)
      }

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
      const { projectId } = await assertIssueAccess(
        ctx.session.user.id,
        input.issueId,
        `write`
      )

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

      const repo = await resolveProjectRepository(projectId)
      if (!repo) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `No repository is connected to this project`,
        })
      }
      if (!githubAppConfigured()) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not configured on this instance`,
        })
      }
      const token = await resolveRepoInstallationToken(repo.fullName)
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not installed on ${repo.fullName}`,
        })
      }

      try {
        await mergePullRequest({
          repo: repo.fullName,
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
      const escaped = input.query.replace(/[%_\\]/g, (m) => `\\${m}`)
      const like = `%${escaped}%`

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
