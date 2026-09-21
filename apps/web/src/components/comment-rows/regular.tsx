import type { Attachment, Comment, User } from "@/db/schema"
import {
  conceptIcon,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  GlassCard,
  UserAvatar,
  type UserAvatarSize,
} from "@exp/ui"
import { getCommentBodyText } from "@/lib/domain"
import { cn } from "@/lib/utils"
import { CommentComposer } from "@/components/comment-composer"
import { issueMemoryOwner } from "@/lib/work-tab-memory"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { CommentAttachments } from "@/components/comment-rows/attachments"
import { TimelineRow } from "@/components/comment-rows/timeline-row"
import { authorLabel, relativeTime } from "./format"

// EXP-698 r5: the comment menu is a bare vertical ellipsis on every client —
// no glass ring around it.
const UiMoreVerticalIcon = conceptIcon(`ui-more-vertical`)
// EXP-956: the menu rows carry the same glyphs as the native comment menus.
const UiEditIcon = conceptIcon(`ui-edit`)
const UiDeleteIcon = conceptIcon(`ui-delete`)

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
              <DropdownMenuItem onSelect={onEdit}>
                <UiEditIcon />
                Edit
              </DropdownMenuItem>
              <DropdownMenuItem variant="destructive" onSelect={onDelete}>
                <UiDeleteIcon />
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
  size,
  className,
}: {
  author: User | undefined
  userId: string
  size: UserAvatarSize
  className?: string
}) {
  return (
    <UserAvatar
      size={size}
      className={cn(`shrink-0`, className)}
      user={{ id: userId, name: authorLabel(author, userId), image: author?.image }}
    />
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
        <CommentAvatar author={author} userId={comment.authorId} size={28} />
      }
      markerSize={28}
    >
      <GlassCard className="px-3 pt-2.5 pb-3">
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
                  size={20}
                  className="mt-0.5 [&_[data-slot=avatar-fallback]]:text-[10px]"
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
                  // EXP-894: a half-typed reply survives a tab switch.
                  draft={{
                    owner: issueMemoryOwner(comment.issueId),
                    slot: `reply:${comment.id}`,
                  }}
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
      </GlassCard>
    </TimelineRow>
  )
}
