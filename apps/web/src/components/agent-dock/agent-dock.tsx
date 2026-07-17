import { useEffect, useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import type { CodingSession, Issue } from "@/db/schema"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import { useAgentsData, type AgentSessionRow } from "@/hooks/use-agents-data"
import { AgentSessionView } from "@/components/agent-session"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"

// The global agent-coding dock (EXP-106) — an IDE-style bottom strip of every
// running session in the workspace, with at most one expanded live viewer. It's
// the SOLE mount point for AgentSessionView; issue detail and the Agents page
// only call openDock(). Renders nothing when there's nothing to show.

function RunningDot() {
  return (
    <span className="relative flex size-2 shrink-0">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

export function AgentDock({
  workspaceId,
  workspaceSlug,
  currentUserId,
}: {
  workspaceId: string
  workspaceSlug: string
  currentUserId: string
}) {
  const dock = useAgentDock()
  const { running } = useAgentsData(workspaceId)
  const projects = useWorkspaceProjects(workspaceId)

  const expandedId = dock?.expandedSessionId ?? null

  // Query the expanded session by id in ANY status, so a session that flips to
  // `ended` while expanded stays visible until the user collapses it.
  const { data: expandedRows } = useLiveQuery(
    (query) =>
      expandedId
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.id, expandedId))
        : undefined,
    [expandedId]
  )
  const expandedSession =
    (((expandedRows ?? []) as CodingSession[])[0] ?? null) || null

  // Resolve the expanded session's issue when it isn't among the running rows
  // (i.e. it ended while expanded) — the running rows already carry the join.
  const runningById = useMemo(
    () => new Map(running.map((row) => [row.session.id, row])),
    [running]
  )
  const needsIssueJoin = Boolean(
    expandedSession &&
      expandedSession.issueId &&
      !runningById.has(expandedSession.id)
  )
  const { data: expandedIssueRows } = useLiveQuery(
    (query) =>
      needsIssueJoin && expandedSession?.issueId
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => eq(i.id, expandedSession.issueId))
        : undefined,
    [needsIssueJoin, expandedSession?.issueId]
  )

  const expandedRow: AgentSessionRow | null = useMemo(() => {
    if (!expandedSession) return null
    const existing = runningById.get(expandedSession.id)
    if (existing) return existing
    const issue = expandedSession.issueId
      ? ((expandedIssueRows ?? [])[0] as Issue | undefined)
      : undefined
    const project = issue
      ? projects.find((p) => p.id === issue.projectId)
      : undefined
    return { session: expandedSession, issue, project, user: undefined }
  }, [expandedSession, runningById, expandedIssueRows, projects])

  // The expanded row vanished entirely (hard-deleted) — auto-collapse.
  useEffect(() => {
    if (expandedId && expandedRows && expandedSession === null) {
      dock?.collapseDock()
    }
  }, [expandedId, expandedRows, expandedSession])

  if (running.length === 0 && !expandedRow) return null

  const tabs =
    expandedRow && !runningById.has(expandedRow.session.id)
      ? [...running, expandedRow]
      : running

  return (
    <div className="sticky bottom-0 z-30 border-t border-border bg-background">
      {expandedRow && (
        <div className="h-[70dvh] md:h-96">
          <AgentSessionView
            key={expandedRow.session.id}
            session={expandedRow.session}
            currentUserId={currentUserId}
            title={<SessionTitle row={expandedRow} workspaceSlug={workspaceSlug} />}
            onCollapse={() => dock?.collapseDock()}
          />
        </div>
      )}
      <div className="flex h-9 items-center overflow-x-auto">
        {tabs.map((row) => (
          <DockTab
            key={row.session.id}
            row={row}
            expanded={expandedId === row.session.id}
            onClick={() =>
              expandedId === row.session.id
                ? dock?.collapseDock()
                : dock?.openDock(row.session.id)
            }
          />
        ))}
      </div>
    </div>
  )
}

function SessionTitle({
  row,
  workspaceSlug,
}: {
  row: AgentSessionRow
  workspaceSlug: string
}) {
  const { session, issue, project } = row
  if (issue && project) {
    return (
      <Link
        to="/t/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier"
        params={{
          workspaceSlug,
          projectSlug: project.slug,
          issueIdentifier: issue.identifier,
        }}
        className="font-mono hover:underline"
      >
        {issue.identifier}
      </Link>
    )
  }
  // Issueless batch row, or an issue-scoped row whose issue hasn't synced yet.
  if (!session.issueId) return <span className="font-mono">Batch</span>
  return <span className="font-mono">{issue?.identifier ?? `Issue syncing…`}</span>
}

function DockTab({
  row,
  expanded,
  onClick,
}: {
  row: AgentSessionRow
  expanded: boolean
  onClick: () => void
}) {
  const { session, issue } = row
  const running = session.status === `running`
  const label = issue?.identifier ?? `Batch`
  return (
    <Button
      variant="ghost"
      onClick={onClick}
      className={cn(
        `h-9 shrink-0 gap-1.5 rounded-none border-r border-border px-3 text-xs font-normal`,
        expanded && `bg-muted`
      )}
    >
      {running ? (
        <RunningDot />
      ) : (
        <span className="size-2 shrink-0 rounded-full bg-muted-foreground/40" />
      )}
      <span className="font-mono">{label}</span>
      {session.deviceLabel && (
        <span className="max-w-[8rem] truncate text-muted-foreground">
          {` · ${session.deviceLabel}`}
        </span>
      )}
    </Button>
  )
}
