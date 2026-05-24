import { HelpCircle } from "lucide-react"
import type { Comment, User } from "@/db/schema"
import { getCommentBodyText } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { authorLabel, relativeTime } from "./format"

export interface QuestionCommentRowProps {
  author: User | undefined
  comment: Comment
}

export function QuestionCommentRow({ author, comment }: QuestionCommentRowProps) {
  const unanswered = !comment.answeredAt
  const name = authorLabel(author, true)
  const bodyText = getCommentBodyText(comment.body)
  return (
    <div
      className={
        unanswered
          ? `flex gap-2.5 py-2 px-2 -mx-2 my-1 rounded-md border border-amber-500/40 bg-amber-500/10`
          : `flex gap-2.5 py-2 px-2 -mx-2 my-1 rounded-md border border-border bg-muted/30`
      }
    >
      <Avatar className="h-7 w-7 shrink-0">
        <AvatarFallback className="text-xs">{getInitials(name)}</AvatarFallback>
      </Avatar>
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-2 text-xs">
          <span className="font-medium text-foreground">{name}</span>
          <span
            className={
              unanswered
                ? `inline-flex items-center gap-1 rounded-full bg-amber-500/20 px-1.5 py-0.5 text-[10px] font-medium text-amber-200`
                : `inline-flex items-center gap-1 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground`
            }
          >
            <HelpCircle className="size-2.5" />
            {unanswered ? `Waiting for your answer` : `Answered`}
          </span>
          <span className="text-muted-foreground">
            {relativeTime(comment.createdAt)}
          </span>
        </div>
        <div className="mt-0.5 text-sm whitespace-pre-wrap break-words text-foreground">
          {bodyText}
        </div>
      </div>
    </div>
  )
}
