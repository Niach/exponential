import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Send } from "lucide-react"
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
import { RegularCommentRow } from "@/components/comment-rows/regular"
import { AGENT_EVENT_TYPES } from "@/components/agent-plan-panel"

interface IssueTimelineProps {
  issue: Issue
  currentUserId: string
  isAdmin?: boolean
  users: User[]
}

// The human comment thread + human activity events (status/assignee/label).
// The agent plan/question lifecycle lives in <AgentPlanPanel> and the agent
// activity events in <AgentActivityFeed>, so they're intentionally excluded
// here — this surface stays a conversation between people.
export function IssueTimeline({
  issue,
  currentUserId,
  isAdmin = false,
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

  const userMap = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  const labelMap = useMemo(
    () => new Map((labels ?? []).map((l) => [l.id, l as Label])),
    [labels]
  )

  const [editingCommentId, setEditingCommentId] = useState<string | null>(null)
  const [draft, setDraft] = useState(``)
  const [submitting, setSubmitting] = useState(false)

  // Human conversation only (the agent plan/question lifecycle is rendered in
  // the Plan Panel, and those no longer live in comments).
  const list = (comments ?? []) as Comment[]

  const assignee = issue.assigneeId ? userMap.get(issue.assigneeId) : undefined
  const agentAssigned =
    Boolean(assignee?.isAgent) &&
    issue.status !== `done` &&
    issue.status !== `cancelled`
  const composerPlaceholder = agentAssigned
    ? `Message the agent — it'll be incorporated on the next run…`
    : `Leave a reply…`

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
      // Human activity only; agent-lifecycle events live in the activity feed.
      ...((events ?? []) as IssueEvent[])
        .filter((e) => !AGENT_EVENT_TYPES.has(e.type))
        .map((e) => ({
          kind: `event` as const,
          at: new Date(e.createdAt).getTime(),
          event: e,
        })),
    ]
    items.sort((a, b) => a.at - b.at)
    return items
  }, [list, events])

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    const trimmed = draft.trim()
    if (!trimmed) return
    setSubmitting(true)
    try {
      await trpc.comments.create.mutate({
        issueId: issue.id,
        body: trimmed,
      })
      setDraft(``)
    } finally {
      setSubmitting(false)
    }
  }

  const handleEditSave = async (commentId: string, nextText: string) => {
    await trpc.comments.update.mutate({
      id: commentId,
      body: nextText,
    })
    setEditingCommentId(null)
  }

  const handleDelete = async (commentId: string) => {
    await trpc.comments.delete.mutate({ id: commentId })
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
        const canModify = comment.authorId === currentUserId || isAdmin
        return (
          <RegularCommentRow
            key={comment.id}
            author={author}
            comment={comment}
            canModify={canModify}
            editing={editingCommentId === comment.id}
            showRetry={false}
            retrying={false}
            onCancelEdit={() => setEditingCommentId(null)}
            onDelete={() => void handleDelete(comment.id)}
            onEdit={() => setEditingCommentId(comment.id)}
            onSaveEdit={(text) => handleEditSave(comment.id, text)}
            onRetry={() => {}}
          />
        )
      })}
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
