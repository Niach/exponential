import type { Attachment, Comment, User } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { getCommentBodyText } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { CommentComposer } from "@/components/comment-composer"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { CommentAttachments } from "@/components/comment-rows/attachments"
import { TimelineRow } from "@/components/comment-rows/timeline-row"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { authorLabel, relativeTime } from "./format"

// EXP-698 r5: the comment menu is a bare vertical ellipsis on every client —
// no glass ring around it.
const UiMoreVerticalIcon = conceptIcon(`ui-more-vertical`)

export interface RegularCommentRowProps {
  author: User | undefined
  comment: Comment
  // Attachments linked to this comment (attachments.comment_id, EXP-554).
  attachments: Attachment[]
  canModify: boolean
  editing: boolean
  /** The feed's LAST row draws no rail below it — see `TimelineRow`. */
  lineBelow?: boolean
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
  lineBelow = true,
  onDelete,
  onEdit,
  onCancelEdit,
  onSaveEdit,
  users,
}: RegularCommentRowProps) {
  const bodyText = getCommentBodyText(comment.body)
  const name = authorLabel(author, comment.authorId)

  // EXP-698 r5: the comment is a BUBBLE — name, time and the ⋮ menu live
  // inside the card with the body, and the avatar rides the timeline gutter
  // (iOS `RegularCommentRow` / Android `RegularCommentRow.kt`).
  return (
    <TimelineRow
      lineBelow={lineBelow}
      marker={
        <Avatar className="h-7 w-7 shrink-0">
          {author?.image && <AvatarImage src={author.image} />}
          <AvatarFallback className="text-xs" userId={comment.authorId}>
            {getInitials(name)}
          </AvatarFallback>
        </Avatar>
      }
      markerSize={28}
    >
      <div className="rounded-xl border border-glass-stroke-card bg-glass-card px-3 pt-2.5 pb-3">
        {/* EXP-723: the name carries the row (body size, medium), the time and
            the edited marker sit back as separate muted spans — a Linear-style
            header line rather than one uniform 12px run. */}
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-foreground">{name}</span>
          <span className="text-xs text-muted-foreground">
            {relativeTime(comment.createdAt)}
          </span>
          {comment.editedAt && (
            <span className="text-xs text-muted-foreground">edited</span>
          )}
          {canModify && !editing && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="-my-1 ml-auto text-muted-foreground hover:text-foreground"
                  aria-label="Comment actions"
                >
                  <UiMoreVerticalIcon />
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
              <div className="mt-1 text-sm text-foreground">
                <MarkdownEditor
                  markdown={bodyText}
                  editable={false}
                  onChange={() => {}}
                  // EXP-698: feed-sized markdown everywhere — zero pad and no
                  // min-height, so the row's own `mt-1` is the only spacing.
                  appearance="chat"
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
    </TimelineRow>
  )
}
