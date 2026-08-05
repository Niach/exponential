import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Send } from "lucide-react"
import type {
  Comment,
  Issue,
  IssueEvent,
  Label,
  Board,
  User,
} from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  commentCollection,
  issueEventCollection,
  labelCollection,
  boardCollection,
} from "@/lib/collections"
import { Button } from "@/components/ui/button"
import { MentionTextarea } from "@/components/mention-textarea"
import { EventRow } from "@/components/comment-rows/event"
import { RegularCommentRow } from "@/components/comment-rows/regular"
import { relativeTime } from "@/components/comment-rows/format"
import { displayUserName } from "@/lib/user-display"

interface IssueTimelineProps {
  issue: Issue
  currentUserId: string
  users: User[]
}

// The comment thread + activity events (status/assignee/label/PR), rendered as
// a Linear-style timeline.
export function IssueTimeline({
  issue,
  currentUserId,
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

  // Board names for board_moved rows (EXP-57).
  const { data: boards } = useLiveQuery((query) =>
    query.from({ boards: boardCollection })
  )

  const userMap = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  const labelMap = useMemo(
    () => new Map((labels ?? []).map((l) => [l.id, l as Label])),
    [labels]
  )
  const boardMap = useMemo(
    () => new Map((boards ?? []).map((p) => [p.id, p as Board])),
    [boards]
  )

  const [editingCommentId, setEditingCommentId] = useState<string | null>(null)
  const [draft, setDraft] = useState(``)
  const [submitting, setSubmitting] = useState(false)

  const list = (comments ?? []) as Comment[]

  const composerPlaceholder = `Leave a reply…`

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

  // Widget-filed issues have no creator (`creatorId` is NULL by design), so
  // they read as filed by the widget itself — the wording every client shares.
  const createdWho =
    issue.source === `widget`
      ? `Feedback widget`
      : displayUserName(
          issue.creatorId ? userMap.get(issue.creatorId) : undefined,
          issue.creatorId
        )
  const createdTime = relativeTime(issue.createdAt)

  // EXP-422 reverses EXP-327: the rule is bounded by the reading column, so
  // the border and the centered body are ONE element (the detail view mounts
  // this inside its `max-w-3xl` column; desktop-app parity).
  return (
    <div className="mx-auto max-w-3xl border-t border-border px-4 py-3">
      <div className="text-sm font-medium text-foreground mb-2">
        Activity {merged.length > 0 ? `(${merged.length})` : ``}
      </div>
      {/* EXP-417: the issue's own creation, synthesized rather than stored —
          natives already show it, and it is what makes an otherwise empty
          timeline read as a history instead of a void. Never part of the
          "(N)" count, which stays the count of real activity. */}
      <div className="flex items-center gap-2 py-1 pl-1 text-xs text-muted-foreground">
        <span className="flex size-3.5 shrink-0 items-center justify-center">
          <span className="size-1.5 rounded-full bg-current" />
        </span>
        <span className="truncate">
          <span className="font-medium text-foreground">{createdWho}</span>
          {` `}created the issue{createdTime ? ` · ${createdTime}` : ``}
        </span>
      </div>
      {merged.map((item) => {
        if (item.kind === `event`) {
          return (
            <EventRow
              key={`e-${item.event.id}`}
              event={item.event}
              userMap={userMap}
              labelMap={labelMap}
              boardMap={boardMap}
            />
          )
        }
        const comment = item.comment
        const author = userMap.get(comment.authorId)
        // Author-only, no global-admin bypass (EXP-398): the server refuses
        // the mutation for anyone else, so offering the menu would only ever
        // be a lie.
        const canModify = comment.authorId === currentUserId
        return (
          <RegularCommentRow
            key={comment.id}
            author={author}
            comment={comment}
            canModify={canModify}
            users={users}
            editing={editingCommentId === comment.id}
            onCancelEdit={() => setEditingCommentId(null)}
            onDelete={() => void handleDelete(comment.id)}
            onEdit={() => setEditingCommentId(comment.id)}
            onSaveEdit={(text) => handleEditSave(comment.id, text)}
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
