import { useState } from "react"
import { MoreHorizontal } from "lucide-react"
import type { Comment, User } from "@/db/schema"
import { getCommentBodyText } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { MentionTextarea } from "@/components/mention-textarea"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { authorLabel, relativeTime } from "./format"

export interface RegularCommentRowProps {
  author: User | undefined
  comment: Comment
  canModify: boolean
  editing: boolean
  onDelete: () => void
  onEdit: () => void
  onCancelEdit: () => void
  onSaveEdit: (text: string) => Promise<void>
  // Team members for the edit composer's @-mention autocomplete.
  users: User[]
}

export function RegularCommentRow({
  author,
  comment,
  canModify,
  editing,
  onDelete,
  onEdit,
  onCancelEdit,
  onSaveEdit,
  users,
}: RegularCommentRowProps) {
  const bodyText = getCommentBodyText(comment.body)
  const name = authorLabel(author, comment.authorId)

  return (
    <div className="flex gap-2.5 py-2">
      <Avatar className="h-7 w-7 shrink-0">
        {author?.image && <AvatarImage src={author.image} />}
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
          <CommentEditComposer
            bodyText={bodyText}
            users={users}
            onCancel={onCancelEdit}
            onSave={onSaveEdit}
          />
        ) : (
          <div className="mt-0.5 text-sm text-foreground">
            <MarkdownEditor markdown={bodyText} editable={false} onChange={() => {}} />
          </div>
        )}
      </div>
    </div>
  )
}

// Mounted only while the row is in edit mode: mounting seeds the draft from
// the body as it is at edit-start (so a remotely synced update is picked up
// instead of a value captured at row mount), and unmounting on cancel
// discards abandoned drafts instead of resurfacing them on the next edit.
function CommentEditComposer({
  bodyText,
  users,
  onCancel,
  onSave,
}: {
  bodyText: string
  users: User[]
  onCancel: () => void
  onSave: (text: string) => Promise<void>
}) {
  const [draft, setDraft] = useState(bodyText)
  const [saving, setSaving] = useState(false)

  const handleSave = async () => {
    const trimmed = draft.trim()
    if (!trimmed || trimmed === bodyText) {
      onCancel()
      return
    }
    setSaving(true)
    try {
      await onSave(trimmed)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="mt-1 space-y-2">
      <MentionTextarea
        autoFocus
        value={draft}
        onValueChange={setDraft}
        users={users}
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
          onClick={onCancel}
          disabled={saving}
        >
          Cancel
        </Button>
      </div>
    </div>
  )
}
