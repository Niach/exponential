import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { formatDistanceToNowStrict } from "date-fns"
import { MoreHorizontal, Send } from "lucide-react"
import type { Comment, User } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { commentCollection } from "@/lib/collections"
import { getCommentBodyText } from "@/lib/domain"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { getInitials } from "@/lib/utils"

interface CommentThreadProps {
  currentUserId: string
  isAdmin?: boolean
  issueId: string
  users: User[]
}

interface CommentRowProps {
  author: User | undefined
  comment: Comment
  canModify: boolean
  onDelete: () => void
  onEdit: () => void
  editing: boolean
  onCancelEdit: () => void
  onSaveEdit: (text: string) => Promise<void>
}

function relativeTime(date: Date | string | null | undefined): string {
  if (!date) return ``
  const value = typeof date === `string` ? new Date(date) : date
  if (Number.isNaN(value.getTime())) return ``
  return formatDistanceToNowStrict(value, { addSuffix: true })
}

function CommentRow({
  author,
  comment,
  canModify,
  onDelete,
  onEdit,
  editing,
  onCancelEdit,
  onSaveEdit,
}: CommentRowProps) {
  const bodyText = getCommentBodyText(comment.body)
  const [draft, setDraft] = useState(bodyText)
  const [saving, setSaving] = useState(false)

  const name = author?.name || author?.email || `Someone`

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
        <AvatarFallback className="text-xs">
          {getInitials(name)}
        </AvatarFallback>
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
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={onDelete}
                >
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
      </div>
    </div>
  )
}

export function CommentThread({
  currentUserId,
  isAdmin = false,
  issueId,
  users,
}: CommentThreadProps) {
  const { data: comments } = useLiveQuery(
    (query) =>
      query
        .from({ comments: commentCollection })
        .where(({ comments }) => eq(comments.issueId, issueId))
        .orderBy(({ comments }) => comments.createdAt),
    [issueId]
  )

  const userMap = useMemo(
    () => new Map(users.map((u) => [u.id, u])),
    [users]
  )

  const [editingCommentId, setEditingCommentId] = useState<string | null>(null)
  const [draft, setDraft] = useState(``)
  const [submitting, setSubmitting] = useState(false)

  const list = (comments ?? []) as Comment[]

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    const trimmed = draft.trim()
    if (!trimmed) return
    setSubmitting(true)
    try {
      await trpc.comments.create.mutate({
        issueId,
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

  return (
    <div className="border-t border-border px-4 py-3 max-h-72 overflow-y-auto">
      <div className="text-xs font-medium text-muted-foreground mb-2">
        Comments {list.length > 0 ? `(${list.length})` : ``}
      </div>
      {list.length === 0 && (
        <div className="text-xs text-muted-foreground py-1">
          No comments yet. Be the first to add one.
        </div>
      )}
      {list.map((comment) => {
        const canModify =
          comment.authorId === currentUserId || isAdmin
        return (
          <CommentRow
            key={comment.id}
            author={userMap.get(comment.authorId)}
            comment={comment}
            canModify={canModify}
            editing={editingCommentId === comment.id}
            onCancelEdit={() => setEditingCommentId(null)}
            onDelete={() => void handleDelete(comment.id)}
            onEdit={() => setEditingCommentId(comment.id)}
            onSaveEdit={(text) => handleEditSave(comment.id, text)}
          />
        )
      })}
      <form onSubmit={handleSubmit} className="mt-2 flex items-end gap-2">
        <Textarea
          placeholder="Write a comment…"
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
