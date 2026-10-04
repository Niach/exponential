import {
  BUILTIN_PRIORITY_COLOR_CLASS,
  BUILTIN_STATUS_COLOR_CLASS,
  Button,
  GlassSectionHeader,
  Pill,
  StatusGlyph,
  conceptIcon,
} from "@exp/ui"
import {
  PRIORITY_LABEL,
  STATUS_LABEL,
  normalizePriority,
  normalizeStatus,
  priorityIcon,
  prConcept,
  statusIcon,
  type IssueComment,
  type IssueDetail,
} from "./model"
import { Markdown } from "./markdown"

const BackIcon = conceptIcon(`ui-back`)
const ExternalIcon = conceptIcon(`ui-external-link`)

// EXP-1183 — `exponential_issues_get` as the issue face: identifier + title,
// the status / priority / PR pills, the description and the latest comments
// as read-only GFM (markdown.tsx). "Open" hands the issue's app URL to the
// host.
export function IssueDetailView({
  issue,
  onBack,
  onOpenLink,
}: {
  issue: IssueDetail
  onBack?: () => void
  onOpenLink?: (url: string) => void
}) {
  const status = normalizeStatus(issue.status)
  const priority = normalizePriority(issue.priority)
  const pr = prConcept(issue.prState)
  const PrIcon = pr ? conceptIcon(pr) : null
  // Oldest first, like the issue's activity; the tool returns newest first.
  const comments = [...(issue.recentComments ?? [])].reverse()
  const origin = appOrigin(issue.url)
  return (
    <article className="flex flex-col gap-4 px-4 py-3">
      <header className="flex items-center gap-2">
        {onBack && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Back to the list"
            onClick={onBack}
          >
            <BackIcon />
          </Button>
        )}
        <span className="font-mono text-xs text-muted-foreground">
          {issue.identifier}
        </span>
        {issue.url && onOpenLink && (
          <Button
            variant="glass"
            size="xs"
            className="ml-auto"
            onClick={() => onOpenLink(issue.url as string)}
          >
            <ExternalIcon />
            Open
          </Button>
        )}
      </header>
      <h1 className="text-xl font-semibold leading-tight">{issue.title}</h1>
      <div className="flex flex-wrap items-center gap-1.5">
        <Pill
          leading={
            <StatusGlyph
              icon={statusIcon(status)}
              colorClass={BUILTIN_STATUS_COLOR_CLASS[status]}
            />
          }
        >
          {STATUS_LABEL[status]}
        </Pill>
        <Pill
          leading={
            <StatusGlyph
              icon={priorityIcon(priority)}
              colorClass={BUILTIN_PRIORITY_COLOR_CLASS[priority]}
            />
          }
        >
          {PRIORITY_LABEL[priority]}
        </Pill>
        {PrIcon && issue.prUrl && (
          <Pill
            mode="action"
            leading={<PrIcon />}
            onClick={() => onOpenLink?.(issue.prUrl as string)}
          >
            {issue.prNumber != null ? `#${issue.prNumber}` : `Pull request`}
          </Pill>
        )}
      </div>
      {issue.description?.trim() ? (
        <Markdown
          source={issue.description}
          origin={origin}
          onOpenLink={onOpenLink}
        />
      ) : (
        <p className="text-sm text-muted-foreground">No description.</p>
      )}
      {comments.length > 0 && (
        <section className="-mx-4 flex flex-col">
          <GlassSectionHeader label="Comments" count={comments.length} />
          {comments.map((comment) => (
            <CommentRow
              key={comment.id}
              comment={comment}
              origin={origin}
              onOpenLink={onOpenLink}
            />
          ))}
        </section>
      )}
    </article>
  )
}

function CommentRow({
  comment,
  origin,
  onOpenLink,
}: {
  comment: IssueComment
  origin?: string
  onOpenLink?: (url: string) => void
}) {
  const caption =
    comment.source === `mcp`
      ? `via MCP`
      : comment.source === `reporter`
        ? `reporter`
        : null
  return (
    <div className="flex flex-col gap-1 border-b border-border/30 px-4 py-3">
      <span className="text-xs text-muted-foreground">
        {formatDate(comment.createdAt)}
        {caption && ` · ${caption}`}
      </span>
      <Markdown source={comment.body} origin={origin} onOpenLink={onOpenLink} />
    </div>
  )
}

function formatDate(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ``
  return date.toLocaleString(undefined, {
    month: `short`,
    day: `numeric`,
    hour: `2-digit`,
    minute: `2-digit`,
  })
}

function appOrigin(url: string | undefined): string | undefined {
  if (!url) return undefined
  try {
    return new URL(url).origin
  } catch {
    return undefined
  }
}
