import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { router, authedProcedure, generateTxId, type Context } from "@/lib/trpc"
import {
  attachments,
  codingSessions,
  comments,
  issueEvents,
  issues,
  issueLabels,
  issueRelations,
  issueStatuses,
  issueSubscribers,
  labels,
  notifications,
  boards,
} from "@/db/schema"
import { and, eq, inArray, sql } from "drizzle-orm"
import {
  resolveTeamAccess,
  assertAssigneeInTeam,
  assertIssueAccess,
  assertTeamMember,
  getIssueTeamContext,
  getBoardTeamId,
  getSoleHumanMemberId,
} from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"
import { resolveIssueReference } from "@/lib/issue-resolver"
import { issueWireColumns } from "@/lib/issue-columns"
import {
  closePullRequest,
  diagnoseUnmergeablePr,
  fetchPullFiles,
  GitHubMergeError,
  mergePullRequest,
  resolvePrBaseState,
  retargetPullRequest,
} from "@/lib/integrations/github-pr"
import {
  githubAppConfigured,
  resolveRepoDefaultBranchCached,
  resolveRepoInstallationTokenInfo,
} from "@/lib/integrations/github-app"
import { recordConversionEvent } from "@/lib/conversion/events"
import { isInstallationLinkedToTeam } from "@/lib/trpc/integrations"
import {
  boardBranchOverride,
  repoBranchOverride,
} from "@/lib/trpc/repositories"
import { isNotMergeable, prMergeFailureError } from "@/lib/trpc/pr-merge-error"
import { escapeLikePattern } from "@/lib/like-pattern"
import { applyStatusDerivations } from "@/lib/status-derivations"
import {
  applyPrClosedState,
  applyPrMergeState,
  endMergedPrSessions,
} from "@/lib/integrations/pr-sync"
import {
  CATEGORY_ANCHOR,
  dateOnlySchema,
  getIssueDescriptionText,
  issueDescriptionSchema,
  issuePrioritySchema,
  type IssueStatus,
  issueStatusInputSchema,
} from "@/lib/domain"
import {
  canonicalizeMarkdownImageUrls,
  extractAttachmentIdsFromDescription,
  hasMarkdownImages,
} from "@/lib/storage/issue-attachments"
import {
  collectIssueAttachmentStorageKeysInTx,
  deleteStorageObjects,
} from "@/lib/storage/issue-attachment-cleanup"
import {
  fireAndForgetAssignmentNotify,
  fireAndForgetIssueMentionNotify,
  fireAndForgetReporterResolution,
  fireAndForgetStatusChangeNotify,
} from "@/lib/integrations/notifications"
import {
  claimPrMerge,
  releasePrMergeClaim,
} from "@/lib/integrations/pr-actor-claims"
import { resolveMentions } from "@/lib/integrations/mentions"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import { recordIssueEvent } from "@/lib/integrations/activity"
import {
  loadIssueRelations,
  syncDuplicateMirror,
  syncReferenceRelations,
} from "@/lib/issue-relations"

// Extract `owner/repo` from a GitHub PR URL
// (https://github.com/owner/repo/pull/123). Returns null if it doesn't match.
function repoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
  return match ? match[1] : null
}

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

// A status move (EXP-314): ids + display-name snapshots let clients render
// custom statuses without a lookup that may no longer resolve. The anchor
// enums feed the notification pipeline and enum-keyed subsystems in-process;
// since EXP-544 they are no longer duplicated into the event payload (old
// event ROWS still carry {from,to} and clients keep their read fallback).
interface StatusChange {
  from: string
  to: string
  fromStatusId: string | null
  toStatusId: string | null
  fromName: string | null
  toName: string | null
}

// EXP-314: resolve a client's {status?, statusId?} write into the dual-write
// form. statusId (new clients) loads the team's issue_statuses row and
// derives the builtin ANCHOR enum — builtins anchor to their own key (the
// in_review builtin is why category alone isn't enough), customs to
// CATEGORY_ANCHOR. A bare enum (old clients, MCP, pr-sync callers) passes
// through untouched: the populate_issue_status_id trigger re-anchors
// status_id for it, so this path and the trigger can never disagree.
// Duplicate-category rows are rejected — the duplicate flow stays on the
// enum + duplicateOfId lockstep (pairing invariant).
async function resolveStatusWrite(
  db: Context[`db`] | Tx,
  teamId: string,
  input: { status?: IssueStatus; statusId?: string }
): Promise<{ status?: IssueStatus; statusId?: string }> {
  if (input.statusId === undefined) {
    return input.status !== undefined ? { status: input.status } : {}
  }
  const [row] = await db
    .select({
      id: issueStatuses.id,
      teamId: issueStatuses.teamId,
      category: issueStatuses.category,
      builtinKey: issueStatuses.builtinKey,
    })
    .from(issueStatuses)
    .where(eq(issueStatuses.id, input.statusId))
    .limit(1)
  if (!row || row.teamId !== teamId) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Status must belong to the issue's team`,
    })
  }
  if (row.category === `duplicate`) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Duplicate requires a canonical issue`,
    })
  }
  return {
    status: row.builtinKey ?? CATEGORY_ANCHOR[row.category],
    statusId: row.id,
  }
}

// Status-derived column management moved to lib/status-derivations.ts
// (EXP-319): shared with statuses.delete AND pr-sync's automation writer,
// which cannot import this module (issues.ts already imports pr-sync).

// The per-issue write core shared by update and bulkUpdate: persists
// setValues, records status/assignee activity events (comparing the FINAL
// persisted values), and auto-subscribes a new assignee. Post-commit side
// effects (notification fan-out) are returned to the caller — never executed
// inside the transaction.
async function finalizeIssueUpdateInTx(
  tx: Tx,
  args: {
    issueId: string
    teamId: string
    actorUserId: string
    current: {
      status: string
      statusId: string | null
      boardId: string
      title: string
      priority: string
      assigneeId: string | null
      // EXP-736: the from-side of the issue_relations duplicate mirror. Both
      // callers already read it for applyStatusDerivations.
      duplicateOfId: string | null
    }
    setValues: Record<string, unknown>
  }
): Promise<{
  issue: typeof issues.$inferSelect
  statusChange: StatusChange | null
  previousAssigneeId: string | null
} | null> {
  const { issueId, teamId, actorUserId, current, setValues } = args

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

  // EXP-736: the duplicate relation row is a dual-write of duplicate_of_id, so
  // it is reconciled from the PERSISTED value — a status move off 'duplicate'
  // clears the column through applyStatusDerivations without ever naming it.
  await syncDuplicateMirror(tx, {
    issueId,
    teamId,
    actorUserId,
    previousDuplicateOfId: current.duplicateOfId,
    nextDuplicateOfId: issue.duplicateOfId,
  })

  let statusChange: StatusChange | null = null
  // Compare the (anchor, statusId) PAIR — a move between two custom statuses
  // of the same category keeps the anchor enum stable and would otherwise
  // record no event and fire no notification (EXP-314). issue.statusId is the
  // post-trigger value, so enum-only writes compare their re-anchored row.
  if (current.status !== issue.status || current.statusId !== issue.statusId) {
    const nameIds = [current.statusId, issue.statusId].filter(
      (v): v is string => v != null
    )
    const nameRows = nameIds.length
      ? await tx
          .select({ id: issueStatuses.id, name: issueStatuses.name })
          .from(issueStatuses)
          .where(inArray(issueStatuses.id, nameIds))
      : []
    const nameById = new Map(nameRows.map((row) => [row.id, row.name]))
    statusChange = {
      from: current.status,
      to: issue.status,
      fromStatusId: current.statusId ?? null,
      toStatusId: issue.statusId ?? null,
      // Display-name snapshots for the timeline; a missing row (deleted
      // status, pre-backfill NULL) leaves null and renderers fall back to
      // munging the anchor enum — old event rows keep rendering unchanged.
      fromName: (current.statusId && nameById.get(current.statusId)) || null,
      toName: (issue.statusId && nameById.get(issue.statusId)) || null,
    }
    await recordIssueEvent(tx, {
      issueId,
      teamId,
      actorUserId,
      type: `status_changed`,
      payload: {
        fromStatusId: statusChange.fromStatusId,
        toStatusId: statusChange.toStatusId,
        fromName: statusChange.fromName,
        toName: statusChange.toName,
      },
    })
  }
  if (current.assigneeId !== issue.assigneeId) {
    await recordIssueEvent(tx, {
      issueId,
      teamId,
      actorUserId,
      type: `assignee_changed`,
      payload: { from: current.assigneeId, to: issue.assigneeId },
    })
    if (issue.assigneeId) {
      await ensureSubscribed(tx, {
        issueId,
        userId: issue.assigneeId,
        teamId,
        source: `assignee`,
      })
    }
  }
  // EXP-530: the ONE priority-change detection point — update, bulkUpdate and
  // the MCP tools all funnel through here. Fires no notifications (deliberate,
  // like label events); exists for timelines + automation event triggers.
  if (current.priority !== issue.priority) {
    await recordIssueEvent(tx, {
      issueId,
      teamId,
      actorUserId,
      type: `priority_changed`,
      payload: { from: current.priority, to: issue.priority },
    })
  }

  return {
    issue,
    statusChange,
    previousAssigneeId: current.assigneeId,
  }
}

export const issuesRouter = router({
  create: authedProcedure
    .input(
      z
        .object({
          boardId: z.string().uuid(),
          title: z.string().min(1).max(500),
          status: issueStatusInputSchema.optional(),
          // EXP-314: a team status row id — the precise-status alternative to
          // the anchor enum (resolveStatusWrite derives the enum from it).
          statusId: z.string().uuid().optional(),
          priority: issuePrioritySchema.optional(),
          assigneeId: z.string().nullable().optional(),
          description: issueDescriptionSchema.optional(),
          dueDate: dateOnlySchema.nullable().optional(),
          labelIds: z.array(z.string().uuid()).optional(),
        })
        // A brand-new issue has nothing to dedupe, and create has no canonical
        // issue to pair with — status='duplicate' + duplicateOfId=null breaks
        // the pairing invariant every update path enforces (same refusal as
        // bulkUpdate). The statusId path rejects duplicate-category rows in
        // resolveStatusWrite.
        .refine((i) => i.status !== `duplicate`, {
          message: `Duplicate requires a canonical issue. Create the issue first, then mark it.`,
        })
        .refine((i) => i.status === undefined || i.statusId === undefined, {
          message: `Pass status or statusId, not both`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      const board = await getBoardTeamId(input.boardId)
      await resolveTeamAccess(ctx.session.user.id, board.teamId, `create_issue`)

      // The assignee is INPUT, not the actor — validate it against the
      // issue's team or any member could push-notify arbitrary users.
      if (input.assigneeId != null) {
        await assertAssigneeInTeam(input.assigneeId, board.teamId)
      }

      // EXP-50: in a solo team (exactly one human member) an unassigned
      // issue can only ever be theirs — default-assign that member. An
      // explicit assignee (validated above) always wins; multi-member
      // teams keep the unassigned default.
      const assigneeId =
        input.assigneeId ?? (await getSoleHumanMemberId(board.teamId))

      if (input.description && hasMarkdownImages(input.description)) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Images can only be added after the issue is created`,
        })
      }

      const statusWrite = await resolveStatusWrite(ctx.db, board.teamId, input)
      const status = statusWrite.status ?? `backlog`
      // Born-terminal issues get the same completedAt the update path stamps
      // on a transition into done/cancelled — the done group sorts on it.
      // Anchor-keyed, so a custom completed/cancelled statusId counts too.
      const completedAt =
        status === `done` || status === `cancelled` ? new Date() : null

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [issue] = await tx
          .insert(issues)
          .values({
            boardId: input.boardId,
            // populate_issue_board_context overwrites with board-derived
            // truth; passed to satisfy the NOT NULL insert contract.
            teamId: board.teamId,
            title: input.title,
            status,
            // Omitted for enum/default writes — populate_issue_status_id
            // derives the team's builtin row.
            statusId: statusWrite.statusId,
            priority: input.priority ?? `none`,
            assigneeId,
            description: input.description ?? null,
            dueDate: input.dueDate ?? null,
            completedAt,
            creatorId: ctx.session.user.id,
          })
          .returning()

        if (input.labelIds && input.labelIds.length > 0) {
          const labelRows = await tx
            .select({ id: labels.id, teamId: labels.teamId })
            .from(labels)
            .where(inArray(labels.id, input.labelIds))

          const wrongTeam = labelRows.find(
            (label) => label.teamId !== board.teamId
          )
          if (wrongTeam || labelRows.length !== input.labelIds.length) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Labels must belong to the same team as the board`,
            })
          }

          await tx.insert(issueLabels).values(
            input.labelIds.map((labelId) => ({
              issueId: issue.id,
              labelId,
              teamId: board.teamId,
              boardId: input.boardId,
            }))
          )
        }

        // EXP-530: `created` event — timeline-suppressed on every client, it
        // exists so automation event triggers can watch inserts. Payload
        // carries priority/status/source so trigger filters never join.
        await recordIssueEvent(tx, {
          issueId: issue.id,
          teamId: board.teamId,
          actorUserId: ctx.session.user.id,
          type: `created`,
          payload: {
            status: issue.status,
            statusId: issue.statusId,
            priority: issue.priority,
            source: issue.source,
          },
        })

        // Auto-subscribe the creator (and assignee, if any) so they get inbox
        // activity. Agents are skipped inside ensureSubscribed.
        await ensureSubscribed(tx, {
          issueId: issue.id,
          userId: ctx.session.user.id,
          teamId: board.teamId,
          source: `creator`,
        })
        if (issue.assigneeId) {
          await ensureSubscribed(tx, {
            issueId: issue.id,
            userId: issue.assigneeId,
            teamId: board.teamId,
            source: `assignee`,
          })
        }

        // Description @mentions get the same treatment as comment mentions:
        // auto-subscribe here, issue_mention fan-out after commit.
        const mentionedUserIds = issue.description
          ? await resolveMentions(
              tx,
              getIssueDescriptionText(issue.description),
              board.teamId
            )
          : []
        for (const userId of mentionedUserIds) {
          await ensureSubscribed(tx, {
            issueId: issue.id,
            userId,
            teamId: board.teamId,
            source: `mention`,
          })
        }

        // EXP-736: `#IDENT` tokens in the description become `related` rows
        // with source='reference'. A new issue has no previous text, so this
        // only ever inserts.
        await syncReferenceRelations(tx, {
          issueId: issue.id,
          teamId: board.teamId,
          actorUserId: ctx.session.user.id,
          previousText: ``,
          nextText: issue.description
            ? getIssueDescriptionText(issue.description)
            : ``,
        })

        return { issue, txId, mentionedUserIds }
      })

      // Activation signal (EXP-362), POST-COMMIT on the global handle — never
      // inside the transaction above: recordConversionEvent swallows errors,
      // so a non-conflict insert failure (deadlock, dropped connection) would
      // poison the tx and turn the following COMMIT into a silent ROLLBACK.
      // The once-per-user partial unique index turns every issue after the
      // first into a free conflict-skip, so this hot path never reads before
      // writing; a crash between commit and record just drops one row.
      await recordConversionEvent(ctx.db, {
        name: `first_issue_created`,
        userId: ctx.session.user.id,
        properties: { teamId: board.teamId },
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
      z
        .object({
          // EXP-707: UUID or human identifier ("EXP-42"), like issues.get.
          id: z.string().trim().min(1).max(64),
          title: z.string().min(1).max(500).optional(),
          status: issueStatusInputSchema.optional(),
          // EXP-314: a team status row id — the precise-status alternative to
          // the anchor enum (resolveStatusWrite derives the enum from it).
          statusId: z.string().uuid().optional(),
          priority: issuePrioritySchema.optional(),
          assigneeId: z.string().nullable().optional(),
          description: issueDescriptionSchema.nullable().optional(),
          dueDate: dateOnlySchema.nullable().optional(),
          // Canonical issue this one duplicates. Kept in lockstep with the
          // 'duplicate' status inside the transaction below: marking forces
          // status='duplicate'; unmarking (null) restores backlog; moving to
          // any other status clears the link.
          duplicateOfId: z.string().uuid().nullable().optional(),
        })
        .refine((i) => i.status === undefined || i.statusId === undefined, {
          message: `Pass status or statusId, not both`,
        })
        // Marking a duplicate forces status='duplicate' — a simultaneous
        // precise statusId would write an inconsistent (status, statusId)
        // pair the trigger can't heal (both columns explicitly changed).
        // Unmarking (duplicateOfId: null) MAY combine with statusId.
        .refine((i) => i.statusId === undefined || i.duplicateOfId == null, {
          message: `Pass duplicateOfId or statusId, not both`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      const { id: idOrIdentifier, ...updates } = input
      const id = await resolveIssueReference(
        ctx.session.user.id,
        idOrIdentifier
      )

      const issueContext = await assertIssueAccess(
        ctx.session.user.id,
        id,
        `write`
      )

      // The assignee is INPUT, not the actor — validate it against the
      // issue's team or any member could push-notify arbitrary users.
      // null (unassign) and undefined (untouched) both skip the check.
      if (updates.assigneeId != null) {
        await assertAssigneeInTeam(updates.assigneeId, issueContext.teamId)
      }

      let previousAssigneeId: string | null = null
      let newlyMentionedUserIds: string[] = []
      const { issue, statusChange, txId } = await ctx.db.transaction(async (tx) => {
        const [currentIssue] = await tx
          .select({
            description: issues.description,
            status: issues.status,
            statusId: issues.statusId,
            boardId: issues.boardId,
            title: issues.title,
            priority: issues.priority,
            assigneeId: issues.assigneeId,
            duplicateOfId: issues.duplicateOfId,
          })
          .from(issues)
          .where(eq(issues.id, id))
          .limit(1)
          // FOR UPDATE serializes concurrent updates of the same issue so the
          // transition checks below (status events) never run against a stale
          // snapshot.
          .for(`update`)

        if (!currentIssue) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Issue not found`,
          })
        }

        previousAssigneeId = currentIssue.assigneeId
        const setValues: Record<string, unknown> = { ...updates }
        // Replace the raw status inputs with the resolved dual-write pair —
        // a raw statusId must never reach drizzle unvalidated.
        delete setValues.status
        delete setValues.statusId
        const statusWrite = await resolveStatusWrite(
          tx,
          issueContext.teamId,
          updates
        )
        Object.assign(setValues, statusWrite)

        // A bare status='duplicate' write carries no canonical issue: allow it
        // only as a restatement for an already-linked row, never as a way to
        // mint an orphaned duplicate (clients render no canonical banner and
        // no unmark affordance without duplicateOfId).
        if (
          updates.status === `duplicate` &&
          updates.duplicateOfId === undefined &&
          currentIssue.duplicateOfId === null
        ) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Duplicate requires a canonical issue`,
          })
        }

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
            // The canonical issue must live in the same team, on a board that
            // is neither trashed nor archived — pointing a live issue at a
            // hidden canonical would render as an unresolvable duplicate.
            const [canonical] = await tx
              .select({ teamId: boards.teamId })
              .from(issues)
              .innerJoin(boards, eq(boards.id, issues.boardId))
              .where(and(eq(issues.id, updates.duplicateOfId), boardVisible()))
              .limit(1)
            if (!canonical || canonical.teamId !== issueContext.teamId) {
              throw new TRPCError({
                code: `BAD_REQUEST`,
                message: `Canonical issue must be in the same team`,
              })
            }
            // Enum-only write (the statusId refine above forbids combining) —
            // populate_issue_status_id re-anchors status_id to the team's
            // duplicate builtin.
            setValues.status = `duplicate`
          } else if (
            (statusWrite.status ?? currentIssue.status) === `duplicate`
          ) {
            // Unmarking: 'duplicate' no longer applies. The prior status isn't
            // stored, so restore the neutral default (unless this same write
            // already picked a new status/statusId).
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

          // EXP-297: no attachment GC here. Removing an image from a
          // description only unlinks it from the markdown — the row (and the
          // blob) survive in the issue's Files list until someone deletes it
          // explicitly (attachments.delete / the team storage manager).

          // Description @mentions, delta-based: only members mentioned in the
          // NEW text but not the old one are subscribed + notified, so
          // re-saving a description never re-pings everyone already in it.
          const previouslyMentioned = new Set(
            await resolveMentions(tx, previousText, issueContext.teamId)
          )
          const nextMentioned = await resolveMentions(
            tx,
            nextText,
            issueContext.teamId
          )
          newlyMentionedUserIds = nextMentioned.filter(
            (userId) => !previouslyMentioned.has(userId)
          )
          for (const userId of newlyMentionedUserIds) {
            await ensureSubscribed(tx, {
              issueId: id,
              userId,
              teamId: issueContext.teamId,
              source: `mention`,
            })
          }

          // EXP-736: same delta shape for `#IDENT` references. No
          // excludeCommentId ⇒ the DESCRIPTION is the slot being replaced, so
          // the survivor scan reads nextText plus the comments rather than
          // the (still unwritten) stored description.
          //
          // `description: null` CLEARS the description (the column rides the
          // `{...updates}` spread; the guard above only keeps the canonical
          // rewrite from replacing that NULL with an empty string), so the
          // empty nextText is the truth and every reference the description
          // held is correctly orphaned.
          await syncReferenceRelations(tx, {
            issueId: id,
            teamId: issueContext.teamId,
            actorUserId: ctx.session.user.id,
            previousText,
            nextText,
          })
        }

        if (Object.keys(setValues).length === 0) {
          const [existing] = await tx
            .select()
            .from(issues)
            .where(eq(issues.id, id))
            .limit(1)
          return {
            issue: existing!,
            statusChange: null as StatusChange | null,
            // Nothing changed — no sync barrier to await (EXP-707 envelope
            // rule: txId only when a write happened).
            txId: undefined as number | undefined,
          }
        }

        const txId: number | undefined = await generateTxId(tx)
        const result = await finalizeIssueUpdateInTx(tx, {
          issueId: id,
          teamId: issueContext.teamId,
          actorUserId: ctx.session.user.id,
          current: currentIssue,
          setValues,
        })
        if (!result) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }

        return { issue: result.issue, statusChange: result.statusChange, txId }
      })

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
          ? newlyMentionedUserIds.filter(
              (userId) => userId !== issue.assigneeId
            )
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
          fromStatusId: statusChange.fromStatusId,
          toStatusId: statusChange.toStatusId,
          toName: statusChange.toName,
        })
        // One-way helpdesk: closing a widget-reported issue emails the
        // external reporter once (idempotent via resolvedNotifiedAt).
        fireAndForgetReporterResolution({
          issueId: issue.id,
          toStatus: statusChange.to,
        })
      }

      return { issue, txId }
    }),

  // Move an issue to another board in the SAME team (EXP-57, web-only
  // UI for now). The issue is renumbered in the target board (Linear-style:
  // EXP-42 → ABC-17): the generate_issue_number trigger is INSERT-only, so the
  // next number is allocated here the same way the trigger does it (read the
  // target's current max, then upsert the monotonic issue_number_counters row
  // — the ON CONFLICT row lock serializes concurrent allocations and the
  // GREATEST clamp heals a stale/missing counter row). The denormalized child
  // board_id columns are re-pointed in the same transaction so shape scoping
  // stays truthful — the populate triggers fire on UPDATE OF board_id too
  // (REV2-5) and re-derive board_id + board_deleted_at from the already-moved
  // issue row. PR/branch linkage (pr_url/pr_number/branch) survives
  // untouched; labels are team-scoped, so they survive too.
  move: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        boardId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await assertIssueAccess(
        ctx.session.user.id,
        input.id,
        `write`
      )

      if (issueContext.boardId === input.boardId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issue is already in this board`,
        })
      }

      // 404s trashed targets — an issue must never move into the trash.
      const targetBoard = await getBoardTeamId(input.boardId)
      if (targetBoard.teamId !== issueContext.teamId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issues can only move within their team`,
        })
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        // FOR UPDATE serializes concurrent moves of the same issue AND pairs
        // with the FOR KEY SHARE read in populate_issue_child_board_id so a
        // child row inserted mid-move can never commit with the old
        // board_id (see 0001_triggers.sql §7).
        const [current] = await tx
          .select({
            identifier: issues.identifier,
            boardId: issues.boardId,
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
        // a board_moved event with a stale from-side).
        if (current.boardId === input.boardId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Issue is already in this board`,
          })
        }

        // FOR SHARE + deleted_at/archived_at recheck under the lock (REV-49):
        // the pre-tx getBoardTeamId visibility check races a concurrent
        // boards.delete or boards.archive — their fan-outs cannot see this
        // still-uncommitted move, so without the lock the issue could land on
        // a hidden board with stale NULL mirrors (synced everywhere, then
        // silently purged or stranded). FOR SHARE conflicts with those
        // UPDATEs' FOR NO KEY UPDATE row lock (KEY SHARE would not), so the
        // trash/archive either committed first (404 here) or waits until this
        // move commits and its fan-out heals the mirrors.
        const [target] = await tx
          .select({
            prefix: boards.prefix,
            slug: boards.slug,
            deletedAt: boards.deletedAt,
            archivedAt: boards.archivedAt,
          })
          .from(boards)
          .where(eq(boards.id, input.boardId))
          .limit(1)
          .for(`share`)
        if (!target || target.deletedAt || target.archivedAt) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Board not found`,
          })
        }

        // Allocate the target board's next number exactly like
        // generate_issue_number (0001_triggers.sql).
        const maxResult = await tx.execute(
          sql`SELECT COALESCE(MAX(number), 0) AS current_max FROM issues WHERE board_id = ${input.boardId}`
        )
        const currentMax = Number(
          (maxResult.rows[0] as { current_max: number | string }).current_max
        )
        const counterResult = await tx.execute(sql`
          INSERT INTO issue_number_counters AS c (board_id, counter)
          VALUES (${input.boardId}, ${currentMax} + 1)
          ON CONFLICT (board_id) DO UPDATE
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
            boardId: input.boardId,
            number: nextNumber,
            identifier: nextIdentifier,
          })
          .where(eq(issues.id, input.id))
          .returning()
        if (!moved) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }

        // Re-point the trigger-denormalized board_id on every issue-child
        // table. The populate triggers fire on these UPDATE OF board_id
        // statements and overwrite with issue-derived truth (board_id +
        // board_deleted_at + board_archived_at — the target board is live,
        // rechecked under the FOR SHARE lock above). team_id is unchanged — moves never cross
        // teams. These are bookkeeping rewrites: the update_updated_at and
        // bump_issue_updated_at_from_comment board_id guards keep them from
        // restamping child rows or re-bumping the issue per comment.
        await tx
          .update(comments)
          .set({ boardId: input.boardId })
          .where(eq(comments.issueId, input.id))
        await tx
          .update(attachments)
          .set({ boardId: input.boardId })
          .where(eq(attachments.issueId, input.id))
        await tx
          .update(issueEvents)
          .set({ boardId: input.boardId })
          .where(eq(issueEvents.issueId, input.id))
        await tx
          .update(issueSubscribers)
          .set({ boardId: input.boardId })
          .where(eq(issueSubscribers.issueId, input.id))
        await tx
          .update(issueLabels)
          .set({ boardId: input.boardId })
          .where(eq(issueLabels.issueId, input.id))
        // EXP-736: relation rows are scoped by their SOURCE issue's board, so
        // only the rows this issue owns move with it (the rows where it is the
        // related side stay on the other issue's board).
        await tx
          .update(issueRelations)
          .set({ boardId: input.boardId })
          .where(eq(issueRelations.issueId, input.id))
        await tx
          .update(codingSessions)
          .set({ boardId: input.boardId })
          .where(eq(codingSessions.issueId, input.id))
        await tx
          .update(notifications)
          .set({ boardId: input.boardId })
          .where(eq(notifications.issueId, input.id))

        await recordIssueEvent(tx, {
          issueId: input.id,
          teamId: issueContext.teamId,
          actorUserId: ctx.session.user.id,
          type: `board_moved`,
          payload: {
            fromBoardId: current.boardId,
            toBoardId: input.boardId,
            fromIdentifier: current.identifier,
            toIdentifier: nextIdentifier,
          },
        })

        return { txId, issue: moved, boardSlug: target.slug }
      })
    }),

  // Bulk property write for the multi-select action bar (status / priority /
  // assignee). One team per batch, one transaction, one txId — Electric
  // awaitTxId covers every row version. Stale ids and issues in trashed
  // boards are silently skipped (addIssues precedent); an empty survivor
  // set is a hard error.
  bulkUpdate: authedProcedure
    .input(
      z
        .object({
          // EXP-707: `issueIds` is canonical (matching issueLabels.bulk*);
          // `ids` is a transitional alias for the shipped iOS build — drop it
          // once the next iOS release is out (tracked in the EXP-707 wave).
          issueIds: z.array(z.string().uuid()).min(1).max(200).optional(),
          ids: z.array(z.string().uuid()).min(1).max(200).optional(),
          status: issueStatusInputSchema.optional(),
          // EXP-314: a team status row id — the precise-status alternative to
          // the anchor enum (resolveStatusWrite derives the enum from it).
          statusId: z.string().uuid().optional(),
          priority: issuePrioritySchema.optional(),
          assigneeId: z.string().nullable().optional(),
        })
        .refine((i) => (i.issueIds === undefined) !== (i.ids === undefined), {
          message: `Pass issueIds (or the deprecated ids), not both`,
        })
        .refine(
          (i) =>
            i.status !== undefined ||
            i.statusId !== undefined ||
            i.priority !== undefined ||
            i.assigneeId !== undefined,
          { message: `Nothing to update` }
        )
        .refine((i) => i.status === undefined || i.statusId === undefined, {
          message: `Pass status or statusId, not both`,
        })
        // Bulk duplicate-marking has no canonical-issue picker, and
        // status='duplicate' with duplicateOfId=null breaks the pairing
        // invariant every single-issue path intercepts. The statusId path
        // rejects duplicate-category rows in resolveStatusWrite.
        .refine((i) => i.status !== `duplicate`, {
          message: `Duplicate requires a canonical issue. Mark issues individually.`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      const issueIds = (input.issueIds ?? input.ids)!
      // Eligibility + team resolution only — every value the per-row writes
      // derive from is re-read under FOR UPDATE inside the transaction below.
      const eligible = await ctx.db
        .select({
          id: issues.id,
          teamId: boards.teamId,
        })
        .from(issues)
        .innerJoin(boards, eq(issues.boardId, boards.id))
        .where(and(inArray(issues.id, issueIds), boardVisible()))

      if (eligible.length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `No updatable issues`,
        })
      }
      const teamIds = new Set(eligible.map((row) => row.teamId))
      if (teamIds.size > 1) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issues must belong to one team`,
        })
      }
      const teamId = eligible[0].teamId
      await assertTeamMember(ctx.session.user.id, teamId)

      // The assignee is INPUT, not the actor — validate it against the
      // batch's team or any member could push-notify arbitrary users.
      if (input.assigneeId != null) {
        await assertAssigneeInTeam(input.assigneeId, teamId)
      }

      const patch: Record<string, unknown> = {
        ...(await resolveStatusWrite(ctx.db, teamId, input)),
        ...(input.priority !== undefined ? { priority: input.priority } : {}),
        ...(input.assigneeId !== undefined
          ? { assigneeId: input.assigneeId }
          : {}),
      }

      const eligibleIds = eligible.map((row) => row.id)

      const { txId, results } = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        // Re-read the batch under FOR UPDATE — the eligibility select above is
        // unlocked, so its status/assignee/duplicate values can already be
        // stale (the single-issue path takes the same lock for exactly this
        // reason). Deriving from the locked rows keeps completedAt from
        // clobbering a concurrent completion and keeps the recorded
        // status/assignee event from-values truthful. ORDER BY id gives
        // overlapping batches one lock order; rows missing from the re-read
        // were deleted in the window and are silently skipped, like the
        // eligibility filter promises.
        const locked = await tx
          .select({
            id: issues.id,
            status: issues.status,
            statusId: issues.statusId,
            boardId: issues.boardId,
            title: issues.title,
            priority: issues.priority,
            assigneeId: issues.assigneeId,
            duplicateOfId: issues.duplicateOfId,
          })
          .from(issues)
          .where(inArray(issues.id, eligibleIds))
          .orderBy(issues.id)
          .for(`update`)

        const results: NonNullable<
          Awaited<ReturnType<typeof finalizeIssueUpdateInTx>>
        >[] = []
        for (const row of locked) {
          const setValues: Record<string, unknown> = { ...patch }
          applyStatusDerivations(setValues, row)
          const result = await finalizeIssueUpdateInTx(tx, {
            issueId: row.id,
            teamId,
            actorUserId: ctx.session.user.id,
            current: row,
            setValues,
          })
          // Row gone despite the lock — skip, keep the batch (the eligibility
          // filter promises silent-skip semantics).
          if (result) results.push(result)
        }
        return { txId, results }
      })

      // Fan-out cap: a 200-issue sweep must not fire hundreds of member
      // pushes — skip the per-issue member notifications past 25 UPDATED
      // issues. The widget reporter's resolution email is deliberately NOT
      // capped: it is the external reporter's only close signal, fires at
      // most once per issue ever (atomic resolvedNotifiedAt claim) and has no
      // retry, so a capped bulk close would drop it permanently.
      const notifyMembers = results.length <= 25
      for (const result of results) {
        if (
          notifyMembers &&
          result.previousAssigneeId !== result.issue.assigneeId
        ) {
          fireAndForgetAssignmentNotify({
            issueId: result.issue.id,
            actorUserId: ctx.session.user.id,
            newAssigneeId: result.issue.assigneeId,
            previousAssigneeId: result.previousAssigneeId,
          })
        }
        if (result.statusChange) {
          if (notifyMembers) {
            fireAndForgetStatusChangeNotify({
              issueId: result.issue.id,
              actorUserId: ctx.session.user.id,
              fromStatus: result.statusChange.from,
              toStatus: result.statusChange.to,
              fromStatusId: result.statusChange.fromStatusId,
              toStatusId: result.statusChange.toStatusId,
              toName: result.statusChange.toName,
            })
          }
          fireAndForgetReporterResolution({
            issueId: result.issue.id,
            toStatus: result.statusChange.to,
          })
        }
      }

      return { txId, updated: results.length }
    }),

  // Bulk delete for the multi-select action bar. Same gates as bulkUpdate
  // (write == delete == membership); attachment blobs are reclaimed from S3
  // after commit like the single delete.
  // EXP-707: `issueIds` (was `ids`) — `ids` stays a TRANSITIONAL alias
  // (exactly one of the two, normalized here) like bulkUpdate's; remove once
  // desktop min >= 0.14.29 (EXP-707 rename; desktop 0.14.28 sends the old
  // key).
  bulkDelete: authedProcedure
    .input(
      z
        .object({
          issueIds: z.array(z.string().uuid()).min(1).max(200).optional(),
          ids: z.array(z.string().uuid()).min(1).max(200).optional(),
        })
        .refine((i) => (i.issueIds === undefined) !== (i.ids === undefined), {
          message: `Pass issueIds (or the deprecated ids), not both`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      const issueIds = (input.issueIds ?? input.ids)!
      const eligible = await ctx.db
        .select({ id: issues.id, teamId: boards.teamId })
        .from(issues)
        .innerJoin(boards, eq(issues.boardId, boards.id))
        .where(and(inArray(issues.id, issueIds), boardVisible()))

      if (eligible.length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `No deletable issues`,
        })
      }
      const teamIds = new Set(eligible.map((row) => row.teamId))
      if (teamIds.size > 1) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Issues must belong to one team`,
        })
      }
      await assertTeamMember(ctx.session.user.id, eligible[0].teamId)

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
  // (the symmetric counterpart of the MCP open_pr tool). Merging completes
  // EVERY issue linked to the PR (a batch PR links several to one prUrl):
  // state write + status→done + pr_merged event + notifications all go
  // through the shared applyPrMergeState writer, whose idempotent
  // open→merged guard also absorbs the later webhook delivery for the same
  // merge.
  mergePr: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        // EXP-711: per-merge override of the team's end-sessions-on-merge
        // setting — false keeps every live session on the PR's issues
        // running, true ends them even when the team switched that off.
        endSessions: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }): Promise<{ merged: true }> => {
      // Member-gated issue write (EXP-180: membership is invite-only and
      // every member is trusted — no extra role clamp).
      const { teamId, boardId } = await assertIssueAccess(
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
        // Already merged (e.g. the webhook beat us) — idempotent no-op for
        // the PR itself, but merge always closes (EXP-498): sweep any live
        // sessions the earlier writer missed.
        const linked = await ctx.db
          .select({ id: issues.id })
          .from(issues)
          .where(eq(issues.prUrl, row.prUrl))
        await endMergedPrSessions(
          linked.map((issue) => issue.id),
          input.endSessions
        )
        return { merged: true }
      }
      if (row.prState !== `open`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The pull request is ${row.prState}. Only open pull requests can be merged.`,
        })
      }

      // Merge against the repo the PR actually lives in — derived from prUrl,
      // never the board's CURRENT repository: after a board repo
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
      const resolved = await resolveRepoInstallationTokenInfo(repoFullName)
      if (!resolved) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not installed on ${repoFullName}`,
        })
      }
      // Link-gate (mirrors prFiles): the installation serving this repo must
      // still be claimed by the issue's team — a deliberately severed
      // GitHub connection must not keep authorizing PR writes through an old
      // prUrl.
      if (
        !(await isInstallationLinkedToTeam(teamId, resolved.installationId))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repoFullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`,
        })
      }

      // EXP-494: record the initiator BEFORE the GitHub merge call — the
      // `closed` webhook reliably beats the applyPrMergeState writes below,
      // and without the claim its fan-out degrades to the session-owner
      // fallback (or a fully anonymous broadcast that self-notifies the
      // merger) whenever no coding_sessions row survived. viaAgent marks an
      // MCP-driven merge (the shared-server daemon acts with its owner's
      // key) so attribution can swap to the session's requester.
      claimPrMerge(repoFullName, row.prNumber, {
        userId: ctx.session.user.id,
        viaAgent: ctx.viaMcp === true,
        // EXP-711: the webhook's sweep must honour the same override.
        endSessions: input.endSessions,
      })
      try {
        await mergePullRequest({
          repo: repoFullName,
          prNumber: row.prNumber,
          token: resolved.token,
          commitTitle: `${row.identifier}: ${row.title} (#${row.prNumber})`,
        })
      } catch (err) {
        // The merge did not happen — drop the claim so it can't misattribute
        // a later out-of-band merge of the same PR.
        releasePrMergeClaim(repoFullName, row.prNumber)
        if (err instanceof GitHubMergeError) {
          // "Not mergeable" is actively misleading on a stacked PR whose base
          // is stale (EXP-324): the real fix is a retarget, not another
          // rebase. The diagnosis names the cause AND says whether this is a
          // real content conflict, which decides the error code clients gate
          // their recovery run on (EXP-533).
          let diagnosis = null
          if (isNotMergeable(err)) {
            const defaultBranch =
              (await boardBranchOverride(boardId, repoFullName)) ??
              (await repoBranchOverride(teamId, repoFullName)) ??
              (await resolveRepoDefaultBranchCached(repoFullName))
            diagnosis = defaultBranch
              ? await diagnoseUnmergeablePr({
                  repo: repoFullName,
                  prNumber: row.prNumber,
                  token: resolved.token,
                  defaultBranch,
                })
              : null
          }
          throw prMergeFailureError(err, diagnosis)
        }
        throw err
      }

      // Complete every issue the PR is linked to — not just the clicked one —
      // so a batch PR's siblings don't wait on the webhook echo (self-hosted
      // instances behind NAT may only have the slower polling cron).
      const linked = await ctx.db
        .select({ id: issues.id })
        .from(issues)
        .where(eq(issues.prUrl, row.prUrl))
      const linkedIds = linked.some((issue) => issue.id === input.issueId)
        ? linked.map((issue) => issue.id)
        : [input.issueId, ...linked.map((issue) => issue.id)]
      for (const issueId of linkedIds) {
        await applyPrMergeState({
          issueId,
          prUrl: row.prUrl,
          mergedAt: new Date(),
          actorUserId: ctx.session.user.id,
          actorViaAgent: ctx.viaMcp === true,
          endSessions: input.endSessions,
        })
      }
      // Merge closes by default (EXP-498, team-configurable since EXP-711):
      // applyPrMergeState's claim winner ends sessions in-tx; this sweep
      // backstops the race where the webhook won the claim before this
      // mutation got here.
      await endMergedPrSessions(linkedIds, input.endSessions)

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
      const { teamId } = await assertIssueAccess(
        ctx.session.user.id,
        input.issueId,
        `write`
      )

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
          message: `The pull request is ${row.prState}. Only open pull requests can be closed.`,
        })
      }

      // Close against the repo the PR actually lives in — derived from prUrl,
      // never the board's CURRENT repository (same derivation as mergePr).
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
      const resolved = await resolveRepoInstallationTokenInfo(repoFullName)
      if (!resolved) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not installed on ${repoFullName}`,
        })
      }
      // Link-gate (mirrors prFiles): the installation serving this repo must
      // still be claimed by the issue's team — a deliberately severed
      // GitHub connection must not keep authorizing PR writes through an old
      // prUrl.
      if (
        !(await isInstallationLinkedToTeam(teamId, resolved.installationId))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repoFullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`,
        })
      }

      try {
        await closePullRequest({
          repo: repoFullName,
          prNumber: row.prNumber,
          token: resolved.token,
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

      // Flip every issue the PR is linked to (batch PRs share one prUrl) so
      // the siblings drop out of the Reviews surfaces without waiting on the
      // webhook echo.
      const closedLinked = await ctx.db
        .select({ id: issues.id })
        .from(issues)
        .where(eq(issues.prUrl, row.prUrl))
      const closedIds = closedLinked.some((issue) => issue.id === input.issueId)
        ? closedLinked.map((issue) => issue.id)
        : [input.issueId, ...closedLinked.map((issue) => issue.id)]
      for (const issueId of closedIds) {
        await applyPrClosedState({
          issueId,
          prUrl: row.prUrl,
        })
      }

      return { closed: true }
    }),

  // Change the base branch of the issue's open PR on GitHub (EXP-324). The
  // stacked-PR self-heal: after a parent PR is squash-merged its branch goes
  // stale, and GitHub only auto-retargets children when the base branch is
  // DELETED — we leave it in place. Omitting `base` targets the repo's live
  // default branch (the right call after a squash-merge). Nothing is
  // persisted locally — the DB carries no base column; GitHub stays the
  // source of truth.
  retargetPr: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        base: z.string().min(1).max(255).optional(),
      })
    )
    .mutation(
      async ({ ctx, input }): Promise<{ retargeted: true; base: string }> => {
        const { teamId } = await assertIssueAccess(
          ctx.session.user.id,
          input.issueId,
          `write`
        )

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
        if (row.prState !== `open`) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `The pull request is ${row.prState}. Only open pull requests can be retargeted.`,
          })
        }

        // Retarget against the repo the PR actually lives in — derived from
        // prUrl, never the board's CURRENT repository (same derivation as
        // mergePr).
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
        const resolved = await resolveRepoInstallationTokenInfo(repoFullName)
        if (!resolved) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `GitHub App is not installed on ${repoFullName}`,
          })
        }
        // Link-gate (mirrors mergePr): the installation serving this repo must
        // still be claimed by the issue's team.
        if (
          !(await isInstallationLinkedToTeam(teamId, resolved.installationId))
        ) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `${repoFullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`,
          })
        }

        const base =
          input.base ??
          (await repoBranchOverride(teamId, repoFullName)) ??
          (await resolveRepoDefaultBranchCached(repoFullName))
        if (!base) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `Could not resolve the default branch of ${repoFullName}`,
          })
        }

        try {
          await retargetPullRequest({
            repo: repoFullName,
            prNumber: row.prNumber,
            base,
            token: resolved.token,
          })
        } catch (err) {
          if (err instanceof GitHubMergeError) {
            if (err.status === 422) {
              throw new TRPCError({
                code: `PRECONDITION_FAILED`,
                message: `'${base}' is not a valid base branch on ${repoFullName}`,
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
              message: `GitHub retarget failed: ${err.message}`,
            })
          }
          throw err
        }

        return { retargeted: true, base }
      }
    ),

  // Resolve the LIVE rebase target for a fix-conflicts run on this issue's PR
  // (EXP-324), healing a dead base along the way. The desktop calls this at
  // launch instead of assuming the repo default: a stacked PR rebases onto its
  // real base, and a PR whose base is already merged/closed/gone is retargeted
  // to the default branch right here — deterministically, before the agent
  // spawns. Idempotent: once retargeted, a re-run classifies `default` and
  // no-ops.
  prepareConflictFix: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const { teamId, boardId } = await assertIssueAccess(
        ctx.session.user.id,
        input.issueId,
        `write`
      )

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
      if (row.prState !== `open`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The pull request is ${row.prState}. Only open pull requests can be conflict-fixed.`,
        })
      }

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
      const resolved = await resolveRepoInstallationTokenInfo(repoFullName)
      if (!resolved) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not installed on ${repoFullName}`,
        })
      }
      if (
        !(await isInstallationLinkedToTeam(teamId, resolved.installationId))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repoFullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`,
        })
      }

      // Override-first (EXP-462): a team pinned to `develop` rebases conflict
      // fixes onto `develop`, and a base classified as dead retargets there.
      // The board's own branch (EXP-712) wins over the repo pin.
      const defaultBranch =
        (await boardBranchOverride(boardId, repoFullName)) ??
        (await repoBranchOverride(teamId, repoFullName)) ??
        (await resolveRepoDefaultBranchCached(repoFullName))
      if (!defaultBranch) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Could not resolve the default branch of ${repoFullName}`,
        })
      }

      let state
      try {
        state = await resolvePrBaseState({
          repo: repoFullName,
          prNumber: row.prNumber,
          token: resolved.token,
          defaultBranch,
        })
      } catch (err) {
        throw new TRPCError({
          code: `BAD_GATEWAY`,
          message:
            err instanceof Error
              ? err.message
              : `Failed to read the pull request from GitHub`,
        })
      }
      if (state.prState !== `open`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The pull request is already ${state.merged ? `merged` : `closed`} on GitHub`,
        })
      }

      let retargeted = false
      if (state.retargetTo != null) {
        try {
          await retargetPullRequest({
            repo: repoFullName,
            prNumber: row.prNumber,
            base: state.retargetTo,
            token: resolved.token,
          })
          retargeted = true
        } catch (err) {
          // 422 = already retargeted (a concurrent heal won the race) —
          // proceed; the rebase target is right either way.
          if (!(err instanceof GitHubMergeError && err.status === 422)) {
            throw new TRPCError({
              code: `BAD_GATEWAY`,
              message: `GitHub retarget failed: ${err instanceof Error ? err.message : `unknown error`}`,
            })
          }
        }
      }

      return {
        repo: repoFullName,
        prNumber: row.prNumber,
        headRef: state.headRef,
        baseRef: state.baseRef,
        baseKind: state.kind,
        rebaseOnto: state.rebaseOnto,
        retargeted,
        defaultBranch,
      }
    }),

  // Changed files for the issue's PR (one issue = one PR), for the diff view.
  // Fetched from GitHub server-side; see lib/integrations/github-pr.ts for the
  // token/visibility caveat.
  prFiles: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      // PR diffs can expose private-repo file contents — member-only (like
      // every read since EXP-180 removed anonymous access).
      const { teamId } = await getIssueTeamContext(input.issueId)
      await assertTeamMember(ctx.session.user.id, teamId)

      const [row] = await ctx.db
        .select({
          prNumber: issues.prNumber,
          prUrl: issues.prUrl,
        })
        .from(issues)
        .where(eq(issues.id, input.issueId))
        .limit(1)

      // Derive owner/repo from the PR URL (repos no longer live on boards —
      // they moved to the server-only repositories registry).
      const repo = row?.prUrl ? repoFromPrUrl(row.prUrl) : null
      if (!row?.prNumber || !repo) {
        return { repo: null as string | null, prNumber: null, files: [] }
      }

      // Link-gate (mirrors repositories.installationToken): the installation
      // serving this repo must still be claimed by the issue's team — a
      // deliberately severed GitHub connection must not keep exposing
      // private-repo PR contents through an old prUrl.
      const resolved = await resolveRepoInstallationTokenInfo(repo)
      if (
        resolved &&
        !(await isInstallationLinkedToTeam(teamId, resolved.installationId))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repo} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`,
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

  // Point read of ONE issue by row UUID or human identifier ("EXP-42").
  // EXP-264: the Electric issues shape is the normal delivery path, but a
  // client can be asked to show an issue it has not synced yet — a push tap on
  // a brand-new issue lands on a blank screen until the shape catches up. This
  // is the fallback that fills that row in. Comments are deliberately absent:
  // they arrive through the comments shape.
  get: authedProcedure
    .input(z.object({ id: z.string().trim().min(1).max(64) }))
    .query(async ({ ctx, input }) => {
      const issueId = await resolveIssueReference(
        ctx.session.user.id,
        input.id
      )

      // Membership is the gate (like every read since EXP-180): a foreign-team
      // UUID probe gets FORBIDDEN, a missing or trashed-board issue NOT_FOUND.
      const { teamId } = await assertIssueAccess(
        ctx.session.user.id,
        issueId,
        `read`
      )

      // EXACTLY the issues shape's server-pinned column allowlist, via the
      // shared camelCase mirror (lib/issue-columns.ts, parity-gated) so a
      // client can merge this row into its synced store verbatim. The REV2-5
      // scoping columns (team_id, board_deleted_at) are excluded — teamId
      // rides top-level below instead.
      const [issue] = await ctx.db
        .select(issueWireColumns)
        .from(issues)
        .where(eq(issues.id, issueId))
        .limit(1)

      if (!issue) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
      }

      const labelRows = await ctx.db
        .select({ labelId: issueLabels.labelId })
        .from(issueLabels)
        .where(eq(issueLabels.issueId, issueId))

      return {
        issue,
        labelIds: labelRows.map((row) => row.labelId),
        // EXP-736: BOTH sides of the relation graph in one read, already
        // folded to this issue's point of view — `direction` says which label
        // half to render, `otherIssueId`/`otherIdentifier` name the far side.
        // The issue_relations shape delivers the same rows continuously; this
        // is the same catch-up fallback the labels above are.
        relations: await loadIssueRelations(ctx.db, issueId),
        // Top-level, NOT a field of `issue`: clients need it to write the
        // denormalized team_id their local issue_labels rows carry, and it is
        // not part of the synced issue row.
        teamId,
      }
    }),

  // Full-text issue search (EXP-3): Postgres FTS over issue title +
  // description AND comment bodies, team-scoped, relevance-ordered. A
  // title/identifier ILIKE fallback keeps this a strict superset of the old
  // title-substring search — it still matches identifiers (EXP-42) and
  // partial title words that FTS lexemes miss. All values are parameterized
  // via drizzle `sql` interpolation; the query cap (REV-17) bounds the FTS
  // parse and LIKE pattern cost per call.
  //
  // REV-14: each `matches` branch is index-friendly on its own — the FTS
  // branches hit the GIN expression indexes (idx_issues_fts /
  // idx_comments_body_fts; the tsvector expressions must stay byte-identical
  // to the index definitions in @exp/db-schema) and the ILIKE branch touches
  // only the cheap title/identifier columns behind idx_issues_team. A single
  // OR'd predicate would force a per-row to_tsvector over every issue in the
  // team instead. Description/comment-body substring fallbacks were dropped
  // deliberately: they required detoasting + scanning megabytes of markdown
  // per keystroke, and whole-word matches ride the FTS branches. Team scoping
  // uses the trigger-denormalized issues/comments team_id +
  // board_deleted_at + board_archived_at mirrors (REV2-5, EXP-500) —
  // equivalent to joining boards on team_id/deleted_at/archived_at, without
  // the join. This is what keeps an archived board's issues and comments out
  // of search on every client.
  search: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        query: z.string().trim().min(1).max(256),
        limit: z.number().int().min(1).max(50).default(20),
      })
    )
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)

      // Escape LIKE wildcards so the substring fallback matches literally.
      const like = `%${escapeLikePattern(input.query)}%`

      const result = await ctx.db.execute(sql`
        with matches as (
          select i.id from issues i
          where i.team_id = ${input.teamId}::uuid
            and i.board_deleted_at is null
            and i.board_archived_at is null
            and to_tsvector('english', coalesce(i.title, '') || ' ' || coalesce(i.description, ''))
              @@ websearch_to_tsquery('english', ${input.query})
          union
          select c.issue_id from comments c
          where c.team_id = ${input.teamId}::uuid
            and c.board_deleted_at is null
            and c.board_archived_at is null
            and to_tsvector('english', c.body) @@ websearch_to_tsquery('english', ${input.query})
          union
          select i.id from issues i
          where i.team_id = ${input.teamId}::uuid
            and i.board_deleted_at is null
            and i.board_archived_at is null
            and (i.title ilike ${like} or i.identifier ilike ${like})
        )
        select
          i.id,
          i.identifier,
          i.title,
          i.board_id as "boardId",
          i.status,
          i.status_id as "statusId",
          i.priority
        from issues i
        join matches m on m.id = i.id
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
        boardId: row.boardId as string,
        status: row.status as string,
        statusId: (row.statusId as string | null) ?? null,
        priority: row.priority as string,
      }))
    }),

  delete: authedProcedure
    // EXP-707: UUID or human identifier ("EXP-42"), like issues.get.
    .input(z.object({ id: z.string().trim().min(1).max(64) }))
    .mutation(async ({ ctx, input }) => {
      const issueId = await resolveIssueReference(
        ctx.session.user.id,
        input.id
      )
      await assertIssueAccess(ctx.session.user.id, issueId, `delete`)

      const storageKeys: Array<string> = []

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        storageKeys.push(
          ...(await collectIssueAttachmentStorageKeysInTx(tx, issueId))
        )

        const deleted = await tx
          .delete(issues)
          .where(eq(issues.id, issueId))
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
