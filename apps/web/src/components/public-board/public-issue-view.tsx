import { useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import { ArrowLeft, CalendarDays } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { TRPCClientError } from "@trpc/client"
import { anonymousUserLabel } from "@/lib/user-display"
import { getStatusConfig, StatusIcon } from "@/components/issue-properties/status-dropdown"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { Badge } from "@/components/ui/badge"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { LiveActivityView } from "./live-activity-view"

type IssueData = Awaited<ReturnType<typeof trpc.publicBoard.issue.query>>

// Read-only public issue page. Author identities are deliberately reduced to
// the deterministic "Member XXXX" handle — the users table never reaches
// public viewers.
export function PublicIssueView({
  workspaceSlug,
  projectSlug,
  issueIdentifier,
}: {
  workspaceSlug: string
  projectSlug: string
  issueIdentifier: string
}) {
  const [data, setData] = useState<IssueData | null>(null)
  const [state, setState] = useState<`loading` | `ready` | `missing`>(
    `loading`
  )

  useEffect(() => {
    let cancelled = false
    setState(`loading`)
    setData(null)
    trpc.publicBoard.issue
      .query({ workspaceSlug, projectSlug, identifier: issueIdentifier })
      .then((result) => {
        if (cancelled) return
        setData(result)
        setState(`ready`)
      })
      .catch((e: unknown) => {
        if (cancelled) return
        if (e instanceof TRPCClientError && e.data?.code === `NOT_FOUND`) {
          setState(`missing`)
        } else {
          throw e
        }
      })
    return () => {
      cancelled = true
    }
  }, [workspaceSlug, projectSlug, issueIdentifier])

  if (state === `loading`) {
    return <p className="text-sm text-muted-foreground">Loading issue…</p>
  }
  if (state === `missing` || !data) {
    return (
      <div className="space-y-3">
        <p className="text-sm text-muted-foreground">
          This issue isn't public (or doesn't exist).
        </p>
        <BackLink workspaceSlug={workspaceSlug} projectSlug={projectSlug} />
      </div>
    )
  }

  const { issue } = data
  const shipped = issue.prState === `merged`

  return (
    <div className="space-y-6">
      <BackLink workspaceSlug={workspaceSlug} projectSlug={projectSlug} />

      <div className="space-y-2">
        <p className="text-xs text-muted-foreground">{issue.identifier}</p>
        <h1 className="text-xl font-semibold">{issue.title}</h1>
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="flex items-center gap-1.5">
            <StatusIcon status={issue.status} />
            {getStatusConfig(issue.status).label}
          </span>
          <span className="flex items-center gap-1.5 text-muted-foreground">
            <PriorityIcon priority={issue.priority} className="h-4 w-4" />
          </span>
          {issue.dueDate && (
            <span className="flex items-center gap-1 text-muted-foreground">
              <CalendarDays className="h-3.5 w-3.5" />
              {issue.dueDate}
            </span>
          )}
          {shipped && (
            <Badge variant="outline" className="text-xs">
              Shipped
            </Badge>
          )}
          {data.labels.map((label) => (
            <Badge
              key={label.id}
              variant="outline"
              className="gap-1 text-xs font-normal"
            >
              <span
                className="h-2 w-2 rounded-full"
                style={{ backgroundColor: label.color }}
              />
              {label.name}
            </Badge>
          ))}
        </div>
      </div>

      {data.codingSession &&
        (data.codingSession.live ? (
          <LiveActivityView
            codingSessionId={data.codingSession.id}
            deviceLabel={data.codingSession.deviceLabel}
          />
        ) : (
          <Badge variant="outline" className="gap-1.5 text-xs">
            <span className="h-2 w-2 animate-pulse rounded-full bg-green-500" />
            Someone is coding this issue right now
            {data.codingSession.deviceLabel
              ? ` on ${data.codingSession.deviceLabel}`
              : ``}
          </Badge>
        ))}

      {issue.description && (
        <div className="rounded-md border border-border/60 p-4">
          <MarkdownEditor
            markdown={issue.description}
            editable={false}
            onChange={() => {}}
          />
        </div>
      )}

      {data.comments.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-medium text-muted-foreground">
            {data.comments.length}{` `}
            {data.comments.length === 1 ? `comment` : `comments`}
          </h2>
          {data.comments.map((comment) => (
            <div
              key={comment.id}
              className="rounded-md border border-border/60 p-3"
            >
              <p className="mb-1 text-xs text-muted-foreground">
                {anonymousUserLabel(comment.authorId)} ·{` `}
                {new Date(comment.createdAt).toLocaleDateString()}
              </p>
              <MarkdownEditor
                markdown={comment.body}
                editable={false}
                onChange={() => {}}
              />
            </div>
          ))}
        </section>
      )}
    </div>
  )
}

function BackLink({
  workspaceSlug,
  projectSlug,
}: {
  workspaceSlug: string
  projectSlug: string
}) {
  return (
    <Link
      to="/w/$workspaceSlug/projects/$projectSlug"
      params={{ workspaceSlug, projectSlug }}
      className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
    >
      <ArrowLeft className="h-3.5 w-3.5" />
      Back to board
    </Link>
  )
}
