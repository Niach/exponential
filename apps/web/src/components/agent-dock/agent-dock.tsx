import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import type { CodingSession, Issue } from "@/db/schema"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useAgentsData, type AgentSessionRow } from "@/hooks/use-agents-data"
import { AgentSessionView } from "@/components/agent-session"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { useIsMobile } from "@/hooks/use-mobile"

// The global agent-coding dock (EXP-106) — an IDE-style bottom strip of every
// running session in the team, with at most one expanded live viewer. It's
// the SOLE mount point for AgentSessionView; issue detail and the Agents page
// only call openDock(). Renders nothing when there's nothing to show.
// Desktop-only chrome (EXP-193): on mobile there is no bottom strip — an
// opened session takes over the viewport like the native apps' pushed
// Agent-session screen, and running sessions are reached from the Agents tab
// (green dot) or the issue's Watch button instead.

function RunningDot() {
  return (
    <span className="relative flex size-2 shrink-0">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

export function AgentDock({
  teamId,
  teamSlug,
  currentUserId,
}: {
  teamId: string
  teamSlug: string
  currentUserId: string
}) {
  const dock = useAgentDock()
  const { running } = useAgentsData(teamId)
  const boards = useTeamBoards(teamId)
  const isMobile = useIsMobile()

  const expandedId = dock?.expandedSessionId ?? null

  // Fullscreen (EXP-184) lives here — outside the per-session `key` remount,
  // so switching dock tabs keeps fullscreen; any collapse exits it.
  const [fullscreen, setFullscreen] = useState(false)
  useEffect(() => {
    if (!expandedId) setFullscreen(false)
  }, [expandedId])

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
    const board = issue
      ? boards.find((p) => p.id === issue.boardId)
      : undefined
    return { session: expandedSession, issue, board, user: undefined }
  }, [expandedSession, runningById, expandedIssueRows, boards])

  // The expanded row vanished entirely (hard-deleted) — auto-collapse.
  useEffect(() => {
    if (expandedId && expandedRows && expandedSession === null) {
      dock?.collapseDock()
    }
  }, [expandedId, expandedRows, expandedSession])

  if (running.length === 0 && !expandedRow) return null

  // Mobile (EXP-193): no IDE-style bottom terminal strip. An opened session
  // is a full-viewport takeover (native Agent-session parity) — its collapse
  // chevron dismisses back to wherever it was opened from; nothing renders
  // while no session is open. z-40 covers the z-[35] floating tab bar while
  // staying under every z-50 overlay (kill-confirm dialog etc.).
  if (isMobile) {
    if (!expandedRow) return null
    return (
      <div className="fixed inset-0 z-40 bg-background pb-[env(safe-area-inset-bottom)]">
        <AgentSessionView
          key={expandedRow.session.id}
          session={expandedRow.session}
          currentUserId={currentUserId}
          title={<SessionTitle row={expandedRow} teamSlug={teamSlug} />}
          onCollapse={() => dock?.collapseDock()}
        />
      </div>
    )
  }

  const tabs =
    expandedRow && !runningById.has(expandedRow.session.id)
      ? [...running, expandedRow]
      : running

  return (
    <div
      className={cn(
        // z-40 covers the layout while staying under every z-50 overlay
        // (dialogs, dropdowns) so kill-confirm etc. still stack above.
        fullscreen && expandedRow
          ? `fixed inset-0 z-40 flex flex-col`
          : `sticky bottom-0 z-30 border-t`,
        `border-border bg-background`
      )}
    >
      {expandedRow && (
        <div className={fullscreen ? `min-h-0 flex-1` : `h-96`}>
          <AgentSessionView
            key={expandedRow.session.id}
            session={expandedRow.session}
            currentUserId={currentUserId}
            title={<SessionTitle row={expandedRow} teamSlug={teamSlug} />}
            onCollapse={() => {
              setFullscreen(false)
              dock?.collapseDock()
            }}
            isFullscreen={fullscreen}
            onToggleFullscreen={() => setFullscreen((f) => !f)}
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
  teamSlug,
}: {
  row: AgentSessionRow
  teamSlug: string
}) {
  const { session, issue, board } = row
  if (issue && board) {
    return (
      <Link
        to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
        params={{
          teamSlug,
          boardSlug: board.slug,
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
      {session.status === `running` ? (
        <RunningDot />
      ) : session.status === `in_review` ? (
        // Agent finished, PR open (EXP-194) — steady sky dot, no ping.
        <span className="size-2 shrink-0 rounded-full bg-sky-500" />
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
