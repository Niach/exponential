import type { Attachment, Comment, User } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { getCommentBodyText } from "@/lib/domain"
import { cn, getInitials } from "@/lib/utils"
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

/**
 * Everything ONE comment needs to render its header, body and edit form —
 * the top-level card and each of its replies (EXP-741) share this shape, so
 * a reply edits, saves and deletes exactly like the card it sits under.
 */
export interface CommentCardProps {
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

export interface RegularCommentRowProps extends CommentCardProps {
  /** The feed's LAST row draws no rail below it — see `TimelineRow`. */
  lineBelow?: boolean
  /** EXP-741: this card's replies, in thread order (`threadComments`). */
  replies?: CommentCardProps[]
  /** EXP-741: the inline reply composer is open under this card. */
  replying?: boolean
  onReply?: () => void
  onCancelReply?: () => void
  onSubmitReply?: (text: string, attachmentIds: string[]) => Promise<void>
}

/** The header line + body/edit form + attachment strip of one comment. */
function CommentCardContent({
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
}: CommentCardProps) {
  const bodyText = getCommentBodyText(comment.body)
  const name = authorLabel(author, comment.authorId)
  return (
    <>
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
        {/* EXP-741: an agent posted it over MCP — the same caption on every
            client, so a bot's words never read as its key owner's. */}
        {comment.source === `mcp` && (
          <span className="text-xs text-muted-foreground">via MCP</span>
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
          <CommentAttachments attachments={attachments} canModify={canModify} />
        </>
      )}
    </>
  )
}

function CommentAvatar({
  author,
  userId,
  className,
}: {
  author: User | undefined
  userId: string
  className: string
}) {
  const name = authorLabel(author, userId)
  return (
    <Avatar className={cn(`shrink-0`, className)}>
      {author?.image && <AvatarImage src={author.image} />}
      <AvatarFallback className="text-xs" userId={userId}>
        {getInitials(name)}
      </AvatarFallback>
    </Avatar>
  )
}

export function RegularCommentRow({
  lineBelow = true,
  replies = [],
  replying = false,
  onReply,
  onCancelReply,
  onSubmitReply,
  ...card
}: RegularCommentRowProps) {
  const { author, comment, users } = card

  // EXP-698 r5: the comment is a BUBBLE — name, time and the ⋮ menu live
  // inside the card with the body, and the avatar rides the timeline gutter
  // (iOS `RegularCommentRow` / Android `RegularCommentRow.kt`).
  //
  // EXP-741: the card is the THREAD — its replies sit indented under the body
  // behind one hairline, each with a 20px avatar, and the "Leave a reply…" row
  // closes every top-level card (the composer opens in its place).
  return (
    <TimelineRow
      lineBelow={lineBelow}
      marker={
        <CommentAvatar
          author={author}
          userId={comment.authorId}
          className="h-7 w-7"
        />
      }
      markerSize={28}
    >
      <div className="rounded-xl border border-glass-stroke-card bg-glass-card px-3 pt-2.5 pb-3">
        <CommentCardContent {...card} />
        {onReply && (
          <div
            data-comment-replies
            className="mt-3 border-t border-glass-stroke-card pt-2"
          >
            {replies.map((reply) => (
              <div
                key={reply.comment.id}
                data-comment-reply
                className="flex gap-2 py-1.5"
              >
                <CommentAvatar
                  author={reply.author}
                  userId={reply.comment.authorId}
                  className="mt-0.5 h-5 w-5 [&_[data-slot=avatar-fallback]]:text-[10px]"
                />
                <div className="min-w-0 flex-1">
                  <CommentCardContent {...reply} />
                </div>
              </div>
            ))}
            {replying ? (
              <div className="pt-1">
                <CommentComposer
                  autoFocus
                  issueId={comment.issueId}
                  users={users}
                  placeholder="Leave a reply…"
                  onCancel={onCancelReply}
                  onSubmit={async (text, attachmentIds) => {
                    await onSubmitReply?.(text, attachmentIds)
                  }}
                />
              </div>
            ) : (
              <Button
                type="button"
                variant="ghost"
                size="xs"
                className="-mx-2 w-[calc(100%+1rem)] justify-start font-normal text-muted-foreground hover:text-foreground"
                onClick={onReply}
              >
                Leave a reply…
              </Button>
            )}
          </div>
        )}
      </div>
    </TimelineRow>
  )
}
