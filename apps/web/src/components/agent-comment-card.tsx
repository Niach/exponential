import { useAppName } from "@/hooks/use-apps"
import { createContext, useContext, useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { useNavigate, useParams } from "@tanstack/react-router"
import { AgentBrandMark, CommentCallCard } from "@exp/ui"
import type { Comment, User } from "@/db/schema"
import { commentCollection } from "@/lib/collections"
import { getCommentBodyText } from "@/lib/domain"
import type { ToolItem } from "@/lib/steer-session-store"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { IssueChip } from "@/components/issue-chip"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import {
  authorLabel,
  commentCaption,
  relativeTime,
} from "@/components/comment-rows/format"

// EXP-1241 (EXP-1259 D14): an `exponential_comments_create` call renders the
// REAL comment — the synced row the result's comment ref names (its body,
// author and time), under "Commented on <issue chip>". Until the row syncs
// (or for a comment this client cannot see) it is the result's own preview:
// the ref's title as the body, the run's owner (whose key posted it), "via
// MCP" and the settle's time. Clicking opens the issue scrolled to the
// comment (`?comment=<id>`, `IssueTimeline`).

/** The contract `expToolDisplay` name of `exponential_comments_create`. */
export const COMMENT_CALL_TOOL = `comments_create`

/** What the transcript knows about the run the call sits in: its agent (the
 *  author row's mark), its owner's name (the comment's author until the row
 *  syncs) and the team's members (the synced author's name). */
export const RunCardsContext = createContext<{
  agent: string | null
  owner: string | null
  users: readonly User[]
}>({ agent: null, owner: null, users: [] })

/** The comment and issue a settled call's preview names. */
export function commentCallTarget(item: ToolItem): {
  commentId: string | null
  commentText: string | null
  issueKey: string | null
} {
  const refs = item.preview?.refs ?? []
  const comment = refs.find((ref) => ref.kind === `comment`)
  const issue = refs.find((ref) => ref.kind === `issue`)
  return {
    commentId: comment?.id ?? item.preview?.id ?? null,
    commentText: comment?.title ?? item.preview?.title ?? null,
    // The result's issue id, else the call's subject (its `issueId` input,
    // an identifier or a uuid).
    issueKey: issue?.identifier ?? issue?.id ?? item.detail?.trim() ?? null,
  }
}

const noop = () => {}

export function CommentCallRow({ item }: { item: ToolItem }) {
  const { agent, owner, users } = useContext(RunCardsContext)
  const issueRefs = useIssueRefs()
  const navigate = useNavigate()
  const { teamSlug } = useParams({ strict: false }) as { teamSlug?: string }
  const target = commentCallTarget(item)
  const { data: rows } = useLiveQuery(
    (query) =>
      target.commentId
        ? query
            .from({ comments: commentCollection })
            .where(({ comments }) => eq(comments.id, target.commentId!))
        : undefined,
    [target.commentId]
  )
  const comment = ((rows ?? []) as Comment[])[0]
  const issue = useMemo(() => {
    if (!issueRefs) return null
    if (comment) return issueRefs.resolveById(comment.issueId)
    if (!target.issueKey) return null
    return (
      issueRefs.resolve(target.issueKey) ?? issueRefs.resolveById(target.issueKey)
    )
  }, [issueRefs, comment, target.issueKey])

  const settled = item.settled === true
  const text = comment ? getCommentBodyText(comment.body) : target.commentText
  const author = comment
    ? authorLabel(
        comment,
        comment.authorId
          ? users.find((user) => user.id === comment.authorId)
          : undefined,
        null
      )
    : owner
  const viaAppName = useAppName(comment?.viaAppId)
  const caption = [
    commentCaption(comment ?? { source: `mcp`, audience: `team` }, viaAppName),
    relativeTime(
      comment
        ? comment.createdAt
        : item.settledAt !== undefined
          ? new Date(item.settledAt)
          : null
    ),
  ]
    .filter((part): part is string => Boolean(part))
    .join(` · `)
  const open =
    issue && target.commentId && teamSlug
      ? () =>
          void navigate({
            to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
            params: {
              teamSlug,
              boardSlug: issue.boardSlug,
              issueIdentifier: issue.identifier,
            },
            search: { comment: target.commentId! },
          })
      : undefined

  return (
    <div className="min-w-0 py-0.5" data-testid="comment-call-row">
      <CommentCallCard
        running={!settled}
        issue={
          issue ? (
            <IssueChip
              issue={issue}
              onClick={() => issueRefs?.open(issue.identifier)}
            />
          ) : target.issueKey ? (
            <span className="font-mono text-muted-foreground">
              {target.issueKey}
            </span>
          ) : null
        }
        author={author}
        mark={<AgentBrandMark agent={agent} className="size-3.5 shrink-0" />}
        caption={caption}
        body={
          settled && text?.trim() ? (
            <MarkdownEditor
              markdown={text}
              editable={false}
              onChange={noop}
              appearance="chat"
            />
          ) : null
        }
        onOpen={open}
      />
    </div>
  )
}
