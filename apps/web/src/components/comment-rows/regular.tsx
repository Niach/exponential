import { Ellipsis } from "lucide-react"
import type { Attachment, Comment, User } from "@/db/schema"
import { getCommentBodyText } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { CommentComposer } from "@/components/comment-composer"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { CommentAttachments } from "@/components/comment-rows/attachments"
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
  // Attachments linked to this comment (attachments.comment_id, EXP-554).
  attachments: Attachment[]
  canModify: boolean
  editing: boolean
  onDelete: () => void
  onEdit: () => void
  onCancelEdit: () => void
  onSaveEdit: (text: string, attachmentIds: string[]) => Promise<void>
  // Team members for the edit composer's @-mention autocomplete.
  users: User[]
}

export function RegularCommentRow({
  author,
  comment,
  attachments,
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
                  <Ellipsis className="size-3" />
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
          // Mounted only while the row is in edit mode: mounting seeds the
          // draft from the body/attachments as they are at edit-start (so a
          // remotely synced update is picked up instead of a value captured at
          // row mount), and unmounting on cancel discards abandoned drafts.
          <div className="mt-1">
            <CommentComposer
              autoFocus
              issueId={comment.issueId}
              users={users}
              initialText={bodyText}
              initialAttachments={attachments}
              onCancel={onCancelEdit}
              onSubmit={onSaveEdit}
            />
          </div>
        ) : (
          <>
            {bodyText.trim().length > 0 && (
              <div className="mt-0.5 text-sm text-foreground">
                <MarkdownEditor
                  markdown={bodyText}
                  editable={false}
                  onChange={() => {}}
                />
              </div>
            )}
            <CommentAttachments
              attachments={attachments}
              canModify={canModify}
            />
          </>
        )}
      </div>
    </div>
  )
}
