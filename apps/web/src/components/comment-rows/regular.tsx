import { useState } from "react"
import { Loader2, MoreHorizontal, RefreshCw } from "lucide-react"
import type { Comment, User } from "@/db/schema"
import { getCommentBodyText } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
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
  showRetry: boolean
  retrying: boolean
  onDelete: () => void
  onEdit: () => void
  onCancelEdit: () => void
  onSaveEdit: (text: string) => Promise<void>
  onRetry: () => void
}

export function RegularCommentRow({
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
}: RegularCommentRowProps) {
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
          <div className="mt-0.5 text-sm text-foreground">
            <MarkdownEditor markdown={bodyText} editable={false} onChange={() => {}} />
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
