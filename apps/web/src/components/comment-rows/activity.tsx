import type { Comment } from "@/db/schema"
import { getCommentBodyText } from "@/lib/domain"
import { relativeTime } from "./format"

export interface ActivityCommentRowProps {
  comment: Comment
}

export function ActivityCommentRow({ comment }: ActivityCommentRowProps) {
  const bodyText = getCommentBodyText(comment.body)
  return (
    <div className="flex items-center gap-2 pl-9 py-0.5 text-xs text-muted-foreground">
      <span
        aria-hidden
        className="inline-block size-1.5 shrink-0 rounded-full bg-indigo-400/50"
      />
      <span className="truncate font-mono">{bodyText}</span>
      <span className="ml-auto shrink-0 tabular-nums opacity-60">
        {relativeTime(comment.createdAt)}
      </span>
    </div>
  )
}
