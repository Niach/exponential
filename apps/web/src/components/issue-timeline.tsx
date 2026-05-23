import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { formatDistanceToNowStrict } from "date-fns"
import {
  Check,
  HelpCircle,
  Loader2,
  MoreHorizontal,
  Pencil,
  RefreshCw,
  Send,
  Sparkles,
} from "lucide-react"
import type { Comment, Issue, User } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { commentCollection } from "@/lib/collections"
import { getCommentBodyText } from "@/lib/domain"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { MarkdownEditor } from "@/components/markdown-editor"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { getInitials } from "@/lib/utils"

interface IssueTimelineProps {
  issue: Issue
  currentUserId: string
  isAdmin?: boolean
  canApprovePlan: boolean
  users: User[]
}

// Regular comments the agent posts when something terminal happened. The
// "implementing" spinner hides as soon as any of these land after approval.
// Error-shaped ones (everything except PR-opened) also get a Retry button.
const TERMINAL_BODY_PATTERNS = [
  /^PR opened:/i,
  /^Tests failed after retry/i,
  /^Agent encountered an error/i,
  /^No GitHub repo linked/i,
  /Companion is not authenticated to GitHub/i,
] as const

const ERROR_BODY_PATTERNS = [
  /^Tests failed after retry/i,
  /^Agent encountered an error/i,
  /^No GitHub repo linked/i,
  /Companion is not authenticated to GitHub/i,
] as const

function isErrorComment(comment: Comment): boolean {
  if (comment.kind !== `regular`) return false
  const body = getCommentBodyText(comment.body)
  return ERROR_BODY_PATTERNS.some((rx) => rx.test(body))
}

function isTerminalComment(comment: Comment): boolean {
  if (comment.kind !== `regular`) return false
  const body = getCommentBodyText(comment.body)
  return TERMINAL_BODY_PATTERNS.some((rx) => rx.test(body))
}

function relativeTime(date: Date | string | null | undefined): string {
  if (!date) return ``
  const value = typeof date === `string` ? new Date(date) : date
  if (Number.isNaN(value.getTime())) return ``
  return formatDistanceToNowStrict(value, { addSuffix: true })
}

function authorLabel(author: User | undefined, isAgent: boolean): string {
  if (isAgent) return author?.name || `Agent`
  return author?.name || author?.email || `Someone`
}

interface RegularRowProps {
  author: User | undefined
  comment: Comment
  canModify: boolean
  editing: boolean
  showRetry: boolean
  retrying: boolean
  onDelete: () => void
  onEdit: () => void
  onCancelEdit: () => void
  onSaveEdit: (text: string) => Promise<void>
  onRetry: () => void
}

function RegularCommentRow({
  author,
  comment,
  canModify,
  editing,
  showRetry,
  retrying,
  onDelete,
  onEdit,
  onCancelEdit,
  onSaveEdit,
  onRetry,
}: RegularRowProps) {
  const bodyText = getCommentBodyText(comment.body)
  const [draft, setDraft] = useState(bodyText)
  const [saving, setSaving] = useState(false)
  const name = authorLabel(author, false)

  const handleSave = async () => {
    const trimmed = draft.trim()
    if (!trimmed || trimmed === bodyText) {
      onCancelEdit()
      return
    }
    setSaving(true)
    try {
      await onSaveEdit(trimmed)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex gap-2.5 py-2">
      <Avatar className="h-7 w-7 shrink-0">
        <AvatarFallback className="text-xs">{getInitials(name)}</AvatarFallback>
      </Avatar>
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-2 text-xs">
          <span className="font-medium text-foreground">{name}</span>
          <span className="text-muted-foreground">
            {relativeTime(comment.createdAt)}
            {comment.editedAt ? ` · edited` : ``}
          </span>
          {canModify && !editing && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="ml-auto text-muted-foreground"
                  aria-label="Comment actions"
                >
                  <MoreHorizontal className="size-3" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onSelect={onEdit}>Edit</DropdownMenuItem>
                <DropdownMenuItem variant="destructive" onSelect={onDelete}>
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>
        {editing ? (
          <div className="mt-1 space-y-2">
            <Textarea
              autoFocus
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              className="min-h-16 text-sm"
              disabled={saving}
            />
            <div className="flex items-center gap-2">
              <Button
                type="button"
                size="xs"
                onClick={() => void handleSave()}
                disabled={saving || !draft.trim()}
              >
                Save
              </Button>
              <Button
                type="button"
                size="xs"
                variant="ghost"
                onClick={onCancelEdit}
                disabled={saving}
              >
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <div className="mt-0.5 text-sm whitespace-pre-wrap break-words text-foreground">
            {bodyText}
          </div>
        )}
        {showRetry && (
          <div className="mt-2 flex items-center gap-2">
            <Button
              type="button"
              size="xs"
              variant="outline"
              onClick={onRetry}
              disabled={retrying}
              className="border-amber-500/40 text-amber-200 hover:bg-amber-500/10"
            >
              {retrying ? (
                <Loader2 className="mr-1 size-3 animate-spin" />
              ) : (
                <RefreshCw className="mr-1 size-3" />
              )}
              Retry
            </Button>
            <span className="text-xs text-muted-foreground">
              re-run the agent on this issue
            </span>
          </div>
        )}
      </div>
    </div>
  )
}

interface QuestionRowProps {
  author: User | undefined
  comment: Comment
}

function QuestionCommentRow({ author, comment }: QuestionRowProps) {
  const unanswered = !comment.answeredAt
  const name = authorLabel(author, true)
  const bodyText = getCommentBodyText(comment.body)
  return (
    <div
      className={
        unanswered
          ? `flex gap-2.5 py-2 px-2 -mx-2 my-1 rounded-md border border-amber-500/40 bg-amber-500/10`
          : `flex gap-2.5 py-2 px-2 -mx-2 my-1 rounded-md border border-border bg-muted/30`
      }
    >
      <Avatar className="h-7 w-7 shrink-0">
        <AvatarFallback className="text-xs">{getInitials(name)}</AvatarFallback>
      </Avatar>
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-2 text-xs">
          <span className="font-medium text-foreground">{name}</span>
          <span
            className={
              unanswered
                ? `inline-flex items-center gap-1 rounded-full bg-amber-500/20 px-1.5 py-0.5 text-[10px] font-medium text-amber-200`
                : `inline-flex items-center gap-1 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground`
            }
          >
            <HelpCircle className="size-2.5" />
            {unanswered ? `Waiting for your answer` : `Answered`}
          </span>
          <span className="text-muted-foreground">
            {relativeTime(comment.createdAt)}
          </span>
        </div>
        <div className="mt-0.5 text-sm whitespace-pre-wrap break-words text-foreground">
          {bodyText}
        </div>
      </div>
    </div>
  )
}

interface ActivityRowProps {
  comment: Comment
}

function ActivityCommentRow({ comment }: ActivityRowProps) {
  const bodyText = getCommentBodyText(comment.body)
  return (
    <div className="flex items-center gap-2 pl-9 py-0.5 text-xs text-muted-foreground">
      <span
        aria-hidden
        className="inline-block size-1.5 shrink-0 rounded-full bg-indigo-400/50"
      />
      <span className="truncate font-mono">{bodyText}</span>
      <span className="ml-auto shrink-0 tabular-nums opacity-60">
        {relativeTime(comment.createdAt)}
      </span>
    </div>
  )
}

interface PlanRowProps {
  author: User | undefined
  comment: Comment
  revision: number
  isLatest: boolean
  issueState: Issue[`agentPlanState`]
  approvedAtForLatest: Date | null
  approvedByName: string | null
  canApprovePlan: boolean
  onApprove: () => void
  onRequestChanges: () => void
  busy: null | `approve` | `request_changes`
}

function PlanCommentRow({
  author,
  comment,
  revision,
  isLatest,
  issueState,
  approvedAtForLatest,
  approvedByName,
  canApprovePlan,
  onApprove,
  onRequestChanges,
  busy,
}: PlanRowProps) {
  const name = authorLabel(author, true)
  const bodyText = getCommentBodyText(comment.body)
  const showButtons =
    isLatest && issueState === `awaiting_approval` && canApprovePlan
  return (
    <div className="flex gap-2.5 py-2 px-3 -mx-2 my-2 rounded-md border border-border bg-card">
      <Avatar className="h-7 w-7 shrink-0">
        <AvatarFallback className="text-xs">{getInitials(name)}</AvatarFallback>
      </Avatar>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 text-xs">
          <Sparkles className="size-3 text-indigo-300" />
          <span className="font-medium text-foreground">{name}</span>
          <span className="text-muted-foreground">
            Plan · rev {revision} · {relativeTime(comment.createdAt)}
          </span>
          {isLatest && issueState === `approved` && approvedAtForLatest && (
            <span className="inline-flex items-center gap-1 rounded-full bg-green-500/15 px-1.5 py-0.5 text-[10px] font-medium text-green-200">
              <Check className="size-2.5" />
              Approved
              {approvedByName ? ` by ${approvedByName}` : ``}
            </span>
          )}
        </div>
        <div className="mt-2 text-sm text-foreground">
          <MarkdownEditor
            markdown={bodyText}
            editable={false}
            onChange={() => {}}
          />
        </div>
        {showButtons && (
          <div className="mt-2 flex items-center gap-2">
            <Button
              type="button"
              size="xs"
              onClick={onApprove}
              disabled={busy !== null}
            >
              {busy === `approve` ? (
                <Loader2 className="mr-1 size-3 animate-spin" />
              ) : (
                <Check className="mr-1 size-3" />
              )}
              Approve
            </Button>
            <Button
              type="button"
              size="xs"
              variant="ghost"
              onClick={onRequestChanges}
              disabled={busy !== null}
            >
              {busy === `request_changes` ? (
                <Loader2 className="mr-1 size-3 animate-spin" />
              ) : (
                <Pencil className="mr-1 size-3" />
              )}
              Request changes
            </Button>
            <span className="text-xs text-muted-foreground">
              or reply below to refine
            </span>
          </div>
        )}
      </div>
    </div>
  )
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

  const userMap = useMemo(
    () => new Map(users.map((u) => [u.id, u])),
    [users]
  )

  const [editingCommentId, setEditingCommentId] = useState<string | null>(null)
  const [draft, setDraft] = useState(``)
  const [submitting, setSubmitting] = useState(false)
  const [planBusy, setPlanBusy] = useState<null | `approve` | `request_changes`>(
    null
  )
  const [retrying, setRetrying] = useState(false)

  const list = (comments ?? []) as Comment[]

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
  const implementing = useMemo(() => {
    if (issue.status === `done` || issue.status === `cancelled`) return false
    const state = issue.agentPlanState
    if (state !== `approved` && state !== `drafting`) return false
    const last = list[list.length - 1]
    if (last && isTerminalComment(last)) return false
    return true
  }, [issue.agentPlanState, issue.status, list])

  // Retry attaches to the most recent error comment, but only when nothing
  // newer has happened on the issue. As soon as the daemon engages again
  // (markStarted flips state to drafting, a new plan lands, any new comment
  // posts), the previous error is "stale" and the button should disappear
  // so the user doesn't double-retry into a re-plan they didn't want.
  const latestErrorCommentId = useMemo(() => {
    const last = list[list.length - 1]
    if (!last) return null
    if (!isTerminalComment(last)) return null
    return isErrorComment(last) ? last.id : null
  }, [list])

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
        Activity {list.length > 0 ? `(${list.length})` : ``}
      </div>
      {list.length === 0 && (
        <div className="text-xs text-muted-foreground py-1">
          No comments yet. Be the first to add one.
        </div>
      )}
      {list.map((comment) => {
        const author = userMap.get(comment.authorId)
        if (comment.kind === `activity`) {
          return <ActivityCommentRow key={comment.id} comment={comment} />
        }
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
        const showRetry =
          comment.id === latestErrorCommentId && canApprovePlan
        return (
          <RegularCommentRow
            key={comment.id}
            author={author}
            comment={comment}
            canModify={canModify}
            editing={editingCommentId === comment.id}
            showRetry={showRetry}
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
      <form onSubmit={handleSubmit} className="mt-2 flex items-end gap-2">
        <Textarea
          placeholder="Leave a reply…"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          className="min-h-16 text-sm flex-1"
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
