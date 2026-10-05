import { useState } from "react"
import {
  Composer,
  ComposerSubmit,
  GlassSectionHeader,
  Textarea,
  UserAvatar,
  cn,
  conceptIcon,
} from "@exp/ui"
import { useMcpActions } from "./actions"
import { Markdown } from "./markdown"
import type { IssueComment } from "./model"
import { commentCaption, formatDate, threadComments } from "./issue-detail-logic"

const LoadingIcon = conceptIcon(`ui-loading`)

/** One `exponential_members_list` row — only what a comment draws. */
export interface IssueMember {
  id: string
  name?: string | null
  email?: string | null
  image?: string | null
}

// EXP-1183 — the issue face's conversation: the one-level thread oldest
// first (replies under their root), each by its author's mark + name and the
// "via MCP" / "reporter" caption ×4, bodies as read-only GFM; the composer
// under it posts through `exponential_comments_create` and hands the new row
// up before the refetch lands.
export function IssueComments({
  issueId,
  comments,
  members,
  origin,
  onOpenLink,
  onPosted,
}: {
  issueId: string
  comments: readonly IssueComment[]
  members: ReadonlyMap<string, IssueMember>
  origin?: string
  onOpenLink?: (url: string) => void
  onPosted: (comment: IssueComment | null) => void
}) {
  const thread = threadComments(comments)
  return (
    <section className="-mx-4 flex flex-col">
      <GlassSectionHeader label="Comments" count={comments.length} />
      {thread.map(({ comment, reply }) => (
        <CommentRow
          key={comment.id}
          comment={comment}
          reply={reply}
          author={comment.authorId ? members.get(comment.authorId) : undefined}
          origin={origin}
          onOpenLink={onOpenLink}
        />
      ))}
      <div className="px-4 pt-3">
        <CommentComposer issueId={issueId} onPosted={onPosted} />
      </div>
    </section>
  )
}

function CommentRow({
  comment,
  reply,
  author,
  origin,
  onOpenLink,
}: {
  comment: IssueComment
  reply: boolean
  author?: IssueMember
  origin?: string
  onOpenLink?: (url: string) => void
}) {
  const caption = commentCaption(comment.source)
  const name =
    comment.source === `reporter`
      ? `Reporter`
      : author?.name || author?.email || `Someone`
  return (
    <div
      className={cn(
        `flex gap-2.5 border-b border-border/30 px-4 py-3`,
        reply && `pl-11`
      )}
    >
      <UserAvatar
        user={author ?? { id: comment.authorId, name }}
        size={reply ? 20 : 24}
        className="mt-0.5 shrink-0"
      />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex min-w-0 items-baseline gap-1.5 text-xs">
          <span className="truncate font-medium text-foreground">{name}</span>
          <span className="shrink-0 text-muted-foreground">
            {formatDate(comment.createdAt)}
            {caption && ` · ${caption}`}
          </span>
        </div>
        <Markdown source={comment.body} origin={origin} onOpenLink={onOpenLink} />
      </div>
    </div>
  )
}

function CommentComposer({
  issueId,
  onPosted,
}: {
  issueId: string
  onPosted: (comment: IssueComment | null) => void
}) {
  const { call } = useMcpActions()
  const [text, setText] = useState(``)
  const [sending, setSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const sendable = text.trim().length > 0 && !sending

  const send = async () => {
    if (!sendable) return
    setSending(true)
    setError(null)
    const result = await call<IssueComment>(`exponential_comments_create`, {
      issueId,
      body: text.trim(),
    })
    setSending(false)
    if (result.kind === `error`) {
      setError(result.message)
      return
    }
    setText(``)
    onPosted(result.data && typeof result.data === `object` && `id` in result.data ? result.data : null)
  }

  return (
    <div className="flex flex-col gap-1">
      <Composer
        submit={
          <ComposerSubmit
            disabled={!sendable}
            aria-label="Send comment"
            title="Send comment (⌘/Ctrl+Enter)"
            onClick={() => void send()}
          >
            {sending ? <LoadingIcon className="!size-5 animate-spin" /> : undefined}
          </ComposerSubmit>
        }
      >
        <Textarea
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="Leave a comment…"
          aria-label="Comment"
          disabled={sending}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return
            if (event.key === `Enter` && (event.metaKey || event.ctrlKey)) {
              event.preventDefault()
              void send()
            }
          }}
          className="max-h-60 min-h-16 border-none bg-transparent text-sm shadow-none focus-visible:border-transparent dark:bg-transparent"
        />
      </Composer>
      {error && (
        <span className="text-xs text-destructive" role="alert">
          {error}
        </span>
      )}
    </div>
  )
}
