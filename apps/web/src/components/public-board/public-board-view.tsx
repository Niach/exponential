import { useEffect, useMemo, useState } from "react"
import { Link, useParams } from "@tanstack/react-router"
import { Megaphone } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { TRPCClientError } from "@trpc/client"
import { issueStatusOrder, type IssueStatus } from "@/lib/domain"
import { getStatusConfig, StatusIcon } from "@/components/issue-properties/status-dropdown"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { PoweredByFooter } from "@/components/workspace/powered-by-footer"
import { PublicIssueView } from "./public-issue-view"

// Read-only public feedback board, rendered for EVERY non-member visitor
// (anonymous or signed-in) of a workspace that hosts a public feedback-board
// project. Data comes from the publicBoard tRPC router — one-shot reads, no
// Electric sync (a signed-in non-member's shapes are membership-scoped and
// would deliver nothing here). This component owns the whole /w/$slug subtree
// in public mode: it switches between board and issue rendering off the URL
// params instead of the normal Outlet.

type BoardData = Awaited<ReturnType<typeof trpc.publicBoard.board.query>>

export function PublicWorkspaceView({
  workspaceSlug,
  isAuthed,
}: {
  workspaceSlug: string
  isAuthed: boolean
}) {
  // strict:false — this renders from the layout route, below which the board
  // and issue child routes may or may not be matched.
  const params = useParams({ strict: false }) as {
    projectSlug?: string
    issueIdentifier?: string
  }

  if (params.projectSlug && params.issueIdentifier) {
    return (
      <PublicShell isAuthed={isAuthed}>
        <PublicIssueView
          workspaceSlug={workspaceSlug}
          projectSlug={params.projectSlug}
          issueIdentifier={params.issueIdentifier}
        />
      </PublicShell>
    )
  }

  return (
    <PublicShell isAuthed={isAuthed}>
      <PublicBoard
        workspaceSlug={workspaceSlug}
        projectSlug={params.projectSlug}
      />
    </PublicShell>
  )
}

function PublicShell({
  isAuthed,
  children,
}: {
  isAuthed: boolean
  children: React.ReactNode
}) {
  return (
    <div className="min-h-screen flex flex-col bg-background">
      <header className="border-b border-border/60">
        <div className="mx-auto flex w-full max-w-3xl items-center justify-between px-4 py-3">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <Megaphone className="h-4 w-4" />
            Public feedback board
          </div>
          {!isAuthed && (
            <Button asChild size="sm" variant="outline">
              <Link to="/auth/login" search={{ redirect: undefined }}>
                Sign in
              </Link>
            </Button>
          )}
        </div>
      </header>
      <main className="mx-auto w-full max-w-3xl flex-1 px-4 py-6">
        {children}
      </main>
      <PoweredByFooter />
    </div>
  )
}

function usePublicBoard(workspaceSlug: string, projectSlug?: string) {
  const [data, setData] = useState<BoardData | null>(null)
  const [state, setState] = useState<`loading` | `ready` | `missing`>(
    `loading`
  )

  useEffect(() => {
    let cancelled = false
    setState(`loading`)
    setData(null)
    const load = async () => {
      try {
        let slug = projectSlug
        if (!slug) {
          // Bare /w/$slug — resolve to the workspace's (first) public board.
          const boards = await trpc.publicBoard.boards.query({ workspaceSlug })
          slug = boards[0]?.projectSlug
          if (!slug) {
            if (!cancelled) setState(`missing`)
            return
          }
        }
        const result = await trpc.publicBoard.board.query({
          workspaceSlug,
          projectSlug: slug,
        })
        if (!cancelled) {
          setData(result)
          setState(`ready`)
        }
      } catch (e) {
        if (cancelled) return
        if (e instanceof TRPCClientError && e.data?.code === `NOT_FOUND`) {
          setState(`missing`)
        } else {
          throw e
        }
      }
    }
    void load()
    return () => {
      cancelled = true
    }
  }, [workspaceSlug, projectSlug])

  return { data, state }
}

function PublicBoard({
  workspaceSlug,
  projectSlug,
}: {
  workspaceSlug: string
  projectSlug?: string
}) {
  const { data, state } = usePublicBoard(workspaceSlug, projectSlug)

  const grouped = useMemo(() => {
    if (!data) return []
    const labelsByIssue = new Map<
      string,
      { labelId: string; name: string; color: string }[]
    >()
    for (const link of data.labelLinks) {
      const list = labelsByIssue.get(link.issueId) ?? []
      list.push(link)
      labelsByIssue.set(link.issueId, list)
    }
    return issueStatusOrder
      .map((status) => ({
        status,
        issues: data.issues.filter((issue) => issue.status === status),
      }))
      .filter((group) => group.issues.length > 0)
      .map((group) => ({ ...group, labelsByIssue }))
  }, [data])

  if (state === `loading`) {
    return <p className="text-sm text-muted-foreground">Loading board…</p>
  }
  if (state === `missing` || !data) {
    return (
      <p className="text-sm text-muted-foreground">
        This board isn't public (or doesn't exist).
      </p>
    )
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold">{data.board.projectName}</h1>
        <p className="text-sm text-muted-foreground">
          {data.board.workspaceName} · {data.issues.length}{` `}
          {data.issues.length === 1 ? `issue` : `issues`}
        </p>
      </div>

      {grouped.map(({ status, issues: groupIssues, labelsByIssue }) => (
        <section key={status}>
          <h2 className="mb-2 flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <StatusIcon status={status} />
            {getStatusConfig(status as IssueStatus).label}
            <span className="text-xs">{groupIssues.length}</span>
          </h2>
          <ul className="divide-y divide-border/60 rounded-md border border-border/60">
            {groupIssues.map((issue) => (
              <li key={issue.id}>
                <Link
                  to="/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier"
                  params={{
                    workspaceSlug,
                    projectSlug: data.board.projectSlug,
                    issueIdentifier: issue.identifier,
                  }}
                  className="flex items-center gap-3 px-3 py-2 hover:bg-accent/50 transition-colors"
                >
                  <PriorityIcon
                    priority={issue.priority}
                    className="h-4 w-4 shrink-0"
                  />
                  <span className="w-16 shrink-0 text-xs text-muted-foreground">
                    {issue.identifier}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {issue.title}
                  </span>
                  <span className="flex shrink-0 items-center gap-1">
                    {(labelsByIssue.get(issue.id) ?? []).map((label) => (
                      <Badge
                        key={label.labelId}
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
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ))}

      {data.issues.length === 0 && (
        <p className="text-sm text-muted-foreground">No issues yet.</p>
      )}
    </div>
  )
}
