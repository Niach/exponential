import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type {
  Attachment,
  Comment,
  Issue,
  IssueEvent,
  Label,
  Board,
  User,
} from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  attachmentCollection,
  commentCollection,
  issueEventCollection,
  labelCollection,
  boardCollection,
} from "@/lib/collections"
import { CommentComposer } from "@/components/comment-composer"
import { EventRow } from "@/components/comment-rows/event"
import { RegularCommentRow } from "@/components/comment-rows/regular"
import { relativeTime } from "@/components/comment-rows/format"
import { displayUserName } from "@/lib/user-display"
import { getCommentBodyText } from "@/lib/domain"

interface IssueTimelineProps {
  issue: Issue
  currentUserId: string
  users: User[]
  /** EXP-568: on phones the composer lives in the floating bottom bar, so the
   *  timeline must not render a second one at the end of the thread. */
  hideComposer?: boolean
}

// The comment thread + activity events (status/assignee/label/PR), rendered as
// a Linear-style timeline.
export function IssueTimeline({
  issue,
  currentUserId,
  users,
  hideComposer = false,
}: IssueTimelineProps) {
  const { data: comments } = useLiveQuery(
    (query) =>
      query
        .from({ comments: commentCollection })
        .where(({ comments }) => eq(comments.issueId, issue.id))
        .orderBy(({ comments }) => comments.createdAt)
        // Equal timestamps must not reorder between syncs (EXP-668).
        .orderBy(({ comments }) => comments.id),
    [issue.id]
  )

  const { data: events } = useLiveQuery(
    (query) =>
      query
        .from({ e: issueEventCollection })
        .where(({ e }) => eq(e.issueId, issue.id))
        .orderBy(({ e }) => e.createdAt)
        .orderBy(({ e }) => e.id),
    [issue.id]
  )

  const { data: labels } = useLiveQuery((query) =>
    query.from({ labels: labelCollection })
  )

  // Board names for board_moved rows (EXP-57).
  const { data: boards } = useLiveQuery((query) =>
    query.from({ boards: boardCollection })
  )

  // Comment attachments (EXP-554): linked rows ride attachments.comment_id,
  // grouped per comment for the strips below each body.
  const { data: issueAttachments } = useLiveQuery(
    (query) =>
      query
        .from({ attachments: attachmentCollection })
        .where(({ attachments }) => eq(attachments.issueId, issue.id)),
    [issue.id]
  )
  const commentAttachmentMap = useMemo(() => {
    const map = new Map<string, Attachment[]>()
    for (const row of (issueAttachments ?? []) as Attachment[]) {
      if (!row.commentId) continue
      const list = map.get(row.commentId) ?? []
      list.push(row)
      map.set(row.commentId, list)
    }
    for (const list of map.values()) {
      list.sort(
        (a, b) =>
          new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime()
      )
    }
    return map
  }, [issueAttachments])

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

  const list = (comments ?? []) as Comment[]

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
      // `created` rows exist for automations (EXP-530) but the timeline
      // already synthesizes the creation line — drop them here so they
      // neither render nor inflate the "(N)" count.
      ...((events ?? []) as IssueEvent[])
        .filter((e) => e.type !== `created`)
        .map((e) => ({
          kind: `event` as const,
          at: new Date(e.createdAt).getTime(),
          event: e,
        })),
    ]
    items.sort((a, b) => a.at - b.at)
    return items
  }, [list, events])

  const handleSubmit = async (body: string, attachmentIds: string[]) => {
    await trpc.comments.create.mutate({
      issueId: issue.id,
      body,
      attachmentIds,
    })
  }

  const handleEditSave = async (
    comment: Comment,
    nextText: string,
    attachmentIds: string[]
  ) => {
    const currentIds = (commentAttachmentMap.get(comment.id) ?? []).map(
      (row) => row.id
    )
    const unchanged =
      nextText === getCommentBodyText(comment.body) &&
      attachmentIds.length === currentIds.length &&
      attachmentIds.every((id) => currentIds.includes(id))
    if (!unchanged) {
      await trpc.comments.update.mutate({
        id: comment.id,
        body: nextText,
        attachmentIds,
      })
    }
    setEditingCommentId(null)
  }

  const handleDelete = async (commentId: string) => {
    await trpc.comments.delete.mutate({ id: commentId })
  }

  // Widget- and agent-filed issues have no creator (`creatorId` is NULL by
  // design), so they read as filed by their origin — the wording every client
  // shares.
  const createdWho =
    issue.source === `widget`
      ? `Feedback widget`
      : issue.source === `agent`
        ? `Agent`
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
            attachments={commentAttachmentMap.get(comment.id) ?? []}
            canModify={canModify}
            users={users}
            editing={editingCommentId === comment.id}
            onCancelEdit={() => setEditingCommentId(null)}
            onDelete={() => void handleDelete(comment.id)}
            onEdit={() => setEditingCommentId(comment.id)}
            onSaveEdit={(text, attachmentIds) =>
              handleEditSave(comment, text, attachmentIds)
            }
          />
        )
      })}
      {!hideComposer && (
        <div className="mt-2">
          <CommentComposer
            issueId={issue.id}
            users={users}
            onSubmit={handleSubmit}
          />
        </div>
      )}
    </div>
  )
}
