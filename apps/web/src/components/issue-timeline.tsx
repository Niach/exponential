import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Loader2, Send } from "lucide-react"
import type { Comment, Issue, IssueEvent, Label, User } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  commentCollection,
  issueEventCollection,
  labelCollection,
} from "@/lib/collections"
import { Button } from "@/components/ui/button"
import { MentionTextarea } from "@/components/mention-textarea"
import { EventRow } from "@/components/comment-rows/event"
import { PlanCommentRow } from "@/components/comment-rows/plan"
import { QuestionCommentRow } from "@/components/comment-rows/question"
import { RegularCommentRow } from "@/components/comment-rows/regular"

interface IssueTimelineProps {
  issue: Issue
  currentUserId: string
  isAdmin?: boolean
  canApprovePlan: boolean
  users: User[]
}

export function IssueTimeline({
  issue,
  currentUserId,
  isAdmin = false,
  canApprovePlan,
  users,
}: IssueTimelineProps) {
  const { data: comments } = useLiveQuery(
    (query) =>
      query
        .from({ comments: commentCollection })
        .where(({ comments }) => eq(comments.issueId, issue.id))
        .orderBy(({ comments }) => comments.createdAt),
    [issue.id]
  )

  const { data: events } = useLiveQuery(
    (query) =>
      query
        .from({ e: issueEventCollection })
        .where(({ e }) => eq(e.issueId, issue.id))
        .orderBy(({ e }) => e.createdAt),
    [issue.id]
  )

  const { data: labels } = useLiveQuery((query) =>
    query.from({ labels: labelCollection })
  )

  const userMap = useMemo(
    () => new Map(users.map((u) => [u.id, u])),
    [users]
  )
  const labelMap = useMemo(
    () => new Map((labels ?? []).map((l) => [l.id, l as Label])),
    [labels]
  )

  const [editingCommentId, setEditingCommentId] = useState<string | null>(null)
  const [draft, setDraft] = useState(``)
  const [submitting, setSubmitting] = useState(false)
  const [planBusy, setPlanBusy] = useState<null | `approve` | `request_changes`>(
    null
  )
  const [retrying, setRetrying] = useState(false)

  const list = (comments ?? []) as Comment[]

  // When an agent is on the issue (and it isn't finished), nudge that a comment
  // becomes steering for the agent's next run (D10).
  const assignee = issue.assigneeId ? userMap.get(issue.assigneeId) : undefined
  const agentAssigned =
    Boolean(assignee?.isAgent) &&
    issue.status !== `done` &&
    issue.status !== `cancelled`
  const composerPlaceholder = agentAssigned
    ? `Message the agent — it'll be incorporated on the next run…`
    : `Leave a reply…`

  // Unified, time-sorted activity: comments interleaved with issue_events.
  type TimelineItem =
    | { kind: `comment`; at: number; comment: Comment }
    | { kind: `event`; at: number; event: IssueEvent }
  const merged = useMemo<TimelineItem[]>(() => {
    const items: TimelineItem[] = [
      ...list.map((c) => ({
        kind: `comment` as const,
        at: new Date(c.createdAt).getTime(),
        comment: c,
      })),
      ...((events ?? []) as IssueEvent[]).map((e) => ({
        kind: `event` as const,
        at: new Date(e.createdAt).getTime(),
        event: e,
      })),
    ]
    items.sort((a, b) => a.at - b.at)
    return items
  }, [list, events])

  // The latest kind='plan' comment hosts the approval buttons. Compute
  // index/revision so each plan row shows its own number.
  const planComments = list.filter((c) => c.kind === `plan`)
  const latestPlanId = planComments.at(-1)?.id ?? null
  const planRevisionById = new Map(
    planComments.map((c, idx) => [c.id, idx + 1])
  )

  const approvedByName = useMemo(() => {
    if (!issue.agentPlanApprovedBy) return null
    const u = users.find((x) => x.id === issue.agentPlanApprovedBy)
    return u?.name || u?.email || null
  }, [issue.agentPlanApprovedBy, users])

  // Show a single "Agent is working" spinner under the timeline whenever the
  // daemon is actively engaged. Two cases:
  //
  // - drafting: daemon called markStarted at the top of produce_plan but no
  //   plan has landed yet. Spinner shows so the user sees something rather
  //   than a static error or a blank timeline.
  // - approved: plan is approved, daemon is in the code stage. Spinner stays
  //   until a terminal comment lands ("PR opened: …", an error path, etc.).
  //
  // Hidden once the latest comment is a terminal/error comment (so the user
  // can act on it without the spinner stealing attention) or the issue is
  // done/cancelled.
  // The newest activity item drives the agent's live state (no comment-body
  // parsing — we read the synced pr_state column + issue_events).
  const lastItem = merged[merged.length - 1]
  const latestIsAgentError =
    lastItem?.kind === `event` && lastItem.event.type === `agent_error`

  // Spinner while the agent is actively working: planning (state='drafting') or
  // coding (state='approved'), and no PR yet, no fresh error, not finished.
  const implementing = useMemo(() => {
    if (issue.status === `done` || issue.status === `cancelled`) return false
    const state = issue.agentPlanState
    if (state !== `approved` && state !== `drafting`) return false
    if (issue.prState) return false
    if (latestIsAgentError) return false
    return true
  }, [issue.agentPlanState, issue.status, issue.prState, latestIsAgentError])

  // Retry shows when the newest activity is an agent_error event (nothing newer
  // has happened — once the agent re-engages, a newer event/comment supersedes).
  const showRetry = latestIsAgentError && canApprovePlan

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    const trimmed = draft.trim()
    if (!trimmed) return
    setSubmitting(true)
    try {
      await trpc.comments.create.mutate({
        issueId: issue.id,
        body: { text: trimmed },
      })
      setDraft(``)
    } finally {
      setSubmitting(false)
    }
  }

  const handleEditSave = async (commentId: string, nextText: string) => {
    await trpc.comments.update.mutate({
      id: commentId,
      body: { text: nextText },
    })
    setEditingCommentId(null)
  }

  const handleDelete = async (commentId: string) => {
    await trpc.comments.delete.mutate({ id: commentId })
  }

  const handleApprove = async () => {
    setPlanBusy(`approve`)
    try {
      await trpc.agentPlan.approvePlan.mutate({ issueId: issue.id })
    } finally {
      setPlanBusy(null)
    }
  }

  const handleRequestChanges = async () => {
    setPlanBusy(`request_changes`)
    try {
      await trpc.agentPlan.requestChanges.mutate({ issueId: issue.id })
    } finally {
      setPlanBusy(null)
    }
  }

  const handleRetry = async () => {
    setRetrying(true)
    try {
      await trpc.agentPlan.retry.mutate({ issueId: issue.id })
    } finally {
      setRetrying(false)
    }
  }

  return (
    <div className="border-t border-border px-4 py-3">
      <div className="text-xs font-medium text-muted-foreground mb-2">
        Activity {merged.length > 0 ? `(${merged.length})` : ``}
      </div>
      {merged.length === 0 && (
        <div className="text-xs text-muted-foreground py-1">
          No activity yet. Be the first to add a comment.
        </div>
      )}
      {merged.map((item) => {
        if (item.kind === `event`) {
          return (
            <EventRow
              key={`e-${item.event.id}`}
              event={item.event}
              userMap={userMap}
              labelMap={labelMap}
            />
          )
        }
        const comment = item.comment
        const author = userMap.get(comment.authorId)
        if (comment.kind === `question`) {
          return (
            <QuestionCommentRow
              key={comment.id}
              author={author}
              comment={comment}
            />
          )
        }
        if (comment.kind === `plan`) {
          return (
            <PlanCommentRow
              key={comment.id}
              author={author}
              comment={comment}
              revision={planRevisionById.get(comment.id) ?? 0}
              isLatest={comment.id === latestPlanId}
              issueState={issue.agentPlanState}
              approvedAtForLatest={issue.agentPlanApprovedAt}
              approvedByName={approvedByName}
              canApprovePlan={canApprovePlan}
              onApprove={() => void handleApprove()}
              onRequestChanges={() => void handleRequestChanges()}
              busy={planBusy}
            />
          )
        }
        const canModify =
          comment.authorId === currentUserId || isAdmin
        return (
          <RegularCommentRow
            key={comment.id}
            author={author}
            comment={comment}
            canModify={canModify}
            editing={editingCommentId === comment.id}
            showRetry={false}
            retrying={retrying}
            onCancelEdit={() => setEditingCommentId(null)}
            onDelete={() => void handleDelete(comment.id)}
            onEdit={() => setEditingCommentId(comment.id)}
            onSaveEdit={(text) => handleEditSave(comment.id, text)}
            onRetry={() => void handleRetry()}
          />
        )
      })}
      {implementing && (
        <div className="mt-2 flex items-center gap-2 rounded-md border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          <Loader2 className="size-3 animate-spin text-indigo-300" />
          <span>
            {issue.agentPlanState === `drafting`
              ? `Agent is working on a plan…`
              : `Agent is implementing the approved plan…`}
          </span>
        </div>
      )}
      {showRetry && (
        <div className="mt-2 flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs">
          <span className="text-muted-foreground">The agent hit an error.</span>
          <Button
            variant="outline"
            size="sm"
            className="h-6 px-2 text-xs"
            disabled={retrying}
            onClick={() => void handleRetry()}
          >
            {retrying ? `Retrying…` : `Retry`}
          </Button>
        </div>
      )}
      <form onSubmit={handleSubmit} className="mt-2 flex items-end gap-2">
        <MentionTextarea
          placeholder={composerPlaceholder}
          value={draft}
          onValueChange={setDraft}
          users={users}
          className="min-h-16 text-sm"
          disabled={submitting}
          onKeyDown={(event) => {
            if (
              event.key === `Enter` &&
              (event.metaKey || event.ctrlKey) &&
              draft.trim()
            ) {
              event.preventDefault()
              void handleSubmit(event)
            }
          }}
        />
        <Button
          type="submit"
          size="icon"
          aria-label="Send comment"
          disabled={submitting || !draft.trim()}
        >
          <Send className="size-4" />
        </Button>
      </form>
    </div>
  )
}
