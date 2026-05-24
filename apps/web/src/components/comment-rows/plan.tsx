import { Check, Loader2, Pencil, Sparkles } from "lucide-react"
import type { Comment, Issue, User } from "@/db/schema"
import { getCommentBodyText } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { authorLabel, relativeTime } from "./format"

export interface PlanCommentRowProps {
  author: User | undefined
  comment: Comment
  revision: number
  isLatest: boolean
  issueState: Issue[`agentPlanState`]
  approvedAtForLatest: Date | null
  approvedByName: string | null
  canApprovePlan: boolean
  onApprove: () => void
  onRequestChanges: () => void
  busy: null | `approve` | `request_changes`
}

export function PlanCommentRow({
  author,
  comment,
  revision,
  isLatest,
  issueState,
  approvedAtForLatest,
  approvedByName,
  canApprovePlan,
  onApprove,
  onRequestChanges,
  busy,
}: PlanCommentRowProps) {
  const name = authorLabel(author, true)
  const bodyText = getCommentBodyText(comment.body)
  const showButtons =
    isLatest && issueState === `awaiting_approval` && canApprovePlan
  return (
    <div className="flex gap-2.5 py-2 px-3 -mx-2 my-2 rounded-md border border-border bg-card">
      <Avatar className="h-7 w-7 shrink-0">
        <AvatarFallback className="text-xs">{getInitials(name)}</AvatarFallback>
      </Avatar>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 text-xs">
          <Sparkles className="size-3 text-indigo-300" />
          <span className="font-medium text-foreground">{name}</span>
          <span className="text-muted-foreground">
            Plan · rev {revision} · {relativeTime(comment.createdAt)}
          </span>
          {isLatest && issueState === `approved` && approvedAtForLatest && (
            <span className="inline-flex items-center gap-1 rounded-full bg-green-500/15 px-1.5 py-0.5 text-[10px] font-medium text-green-200">
              <Check className="size-2.5" />
              Approved
              {approvedByName ? ` by ${approvedByName}` : ``}
            </span>
          )}
        </div>
        <div className="mt-2 text-sm text-foreground">
          <MarkdownEditor
            markdown={bodyText}
            editable={false}
            onChange={() => {}}
          />
        </div>
        {showButtons && (
          <div className="mt-2 flex items-center gap-2">
            <Button
              type="button"
              size="xs"
              onClick={onApprove}
              disabled={busy !== null}
            >
              {busy === `approve` ? (
                <Loader2 className="mr-1 size-3 animate-spin" />
              ) : (
                <Check className="mr-1 size-3" />
              )}
              Approve
            </Button>
            <Button
              type="button"
              size="xs"
              variant="ghost"
              onClick={onRequestChanges}
              disabled={busy !== null}
            >
              {busy === `request_changes` ? (
                <Loader2 className="mr-1 size-3 animate-spin" />
              ) : (
                <Pencil className="mr-1 size-3" />
              )}
              Request changes
            </Button>
            <span className="text-xs text-muted-foreground">
              or reply below to refine
            </span>
          </div>
        )}
      </div>
    </div>
  )
}
