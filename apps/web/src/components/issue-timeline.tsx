import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Send } from "lucide-react"
import type { Comment, Issue, IssueEvent, Label, Project, Release, User } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  commentCollection,
  issueEventCollection,
  labelCollection,
  projectCollection,
  releaseCollection,
} from "@/lib/collections"
import { Button } from "@/components/ui/button"
import { MentionTextarea } from "@/components/mention-textarea"
import { EventRow } from "@/components/comment-rows/event"
import { RegularCommentRow } from "@/components/comment-rows/regular"

interface IssueTimelineProps {
  issue: Issue
  currentUserId: string
  isAdmin?: boolean
  users: User[]
}

// The comment thread + activity events (status/assignee/label/PR), rendered as
// a Linear-style timeline.
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

  const { data: releases } = useLiveQuery((query) =>
    query.from({ releases: releaseCollection })
  )

  // Project names for project_moved rows (EXP-57).
  const { data: projects } = useLiveQuery((query) =>
    query.from({ projects: projectCollection })
  )

  const userMap = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  const labelMap = useMemo(
    () => new Map((labels ?? []).map((l) => [l.id, l as Label])),
    [labels]
  )
  const releaseMap = useMemo(
    () => new Map((releases ?? []).map((r) => [r.id, r as Release])),
    [releases]
  )
  const projectMap = useMemo(
    () => new Map((projects ?? []).map((p) => [p.id, p as Project])),
    [projects]
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
              releaseMap={releaseMap}
              projectMap={projectMap}
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
