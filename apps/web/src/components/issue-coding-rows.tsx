import { useEffect, useMemo, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import {
  ChevronRight,
  GitBranch,
  GitPullRequest,
  Loader2,
  MonitorOff,
  MonitorPlay,
  MonitorUp,
} from "lucide-react"
import type { CodingSession, Issue, Board, User } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import {
  codingSessionCollection,
  teamMemberCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { displayUserName } from "@/lib/user-display"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { PropertyGroup } from "@/components/issue-properties-panel"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { useRemoteCodingStart } from "@/hooks/use-remote-coding-start"
import { StartCodingDialog } from "@/components/start-coding-dialog"

// The coding affordances of the issue detail (EXP-106): a compact "coding now"
// / remote-start control that FOCUSES the global dock (never mounts the live
// viewer itself), plus a PR / pushed-branch row that links to the review-detail
// route. Repo presence + membership + relay availability gate them (the same
// signals the server enforces); everything degrades to nothing when absent.
// EXP-184 split them: IssueCodingControl renders as a sidebar "Agent" property
// group on desktop (variant='sidebar') or the classic full-width row on mobile
// (variant='row'); IssuePrRow always stays a main-column row.

/** PR-state pill — open emerald / merged purple / closed rose / draft secondary. */
export function PrStateBadge({ state }: { state: string | null | undefined }) {
  if (!state) return null
  if (state === `draft`) {
    return (
      <Badge variant="secondary" className="h-5 px-1.5 text-[0.625rem]">
        Draft
      </Badge>
    )
  }
  const styles: Record<string, string> = {
    open: `border-emerald-500/40 text-emerald-400`,
    merged: `border-purple-500/40 text-purple-400`,
    closed: `border-rose-500/40 text-rose-400`,
  }
  const cls = styles[state]
  if (!cls) return null
  return (
    <Badge
      variant="outline"
      className={cn(`h-5 px-1.5 text-[0.625rem] capitalize`, cls)}
    >
      {state}
    </Badge>
  )
}

function RunningPing() {
  return (
    <span className="relative flex size-2">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

/** Static counterpart of RunningPing for `in_review` sessions (EXP-194) —
 * the agent finished and its PR is up; nothing is "running", so no ping. */
function ReviewDot() {
  return <span className="inline-flex size-2 rounded-full bg-sky-500" />
}

/** Live-session badge — "Coding now" (running) or "Ready for review"
 * (in_review). Shared by the issue detail row and the Agents page. */
export function SessionStatusBadge({
  status,
  count = 1,
}: {
  status: string
  count?: number
}) {
  const inReview = status === `in_review`
  return (
    <Badge
      variant="outline"
      className={cn(
        `gap-1.5`,
        inReview
          ? `border-sky-500/40 text-sky-400`
          : `border-emerald-500/40 text-emerald-400`
      )}
    >
      {inReview ? <ReviewDot /> : <RunningPing />}
      {inReview ? `Ready for review` : `Coding now`}
      {count > 1 ? ` (·${count})` : ``}
    </Badge>
  )
}

export type CodingControlVariant = `row` | `sidebar`

// Membership gate shared by both exported pieces (the server enforces it
// regardless; this only decides what renders).
function useIsTeamMember(teamId: string, currentUserId: string) {
  const { data: memberRows } = useLiveQuery(
    (query) =>
      query
        .from({ m: teamMemberCollection })
        .where(({ m }) =>
          and(eq(m.teamId, teamId), eq(m.userId, currentUserId))
        ),
    [teamId, currentUserId]
  )
  return (memberRows?.length ?? 0) > 0
}

/** The "coding now" / remote-start control — sidebar property group or row. */
export function IssueCodingControl({
  issue,
  board,
  teamId,
  currentUserId,
  users,
  variant,
}: {
  issue: Issue
  board: Board
  teamId: string
  currentUserId: string
  users: User[]
  variant: CodingControlVariant
}) {
  const config = useSteerConfig()
  const isMember = useIsTeamMember(teamId, currentUserId)

  return (
    <AgentRow
      issue={issue}
      board={board}
      teamId={teamId}
      users={users}
      isMember={isMember}
      steerEnabled={config?.enabled ?? null}
      variant={variant}
    />
  )
}

/** The PR / pushed-branch main-column row. */
export function IssuePrRow({
  issue,
  board,
  teamId,
  teamSlug,
  currentUserId,
}: {
  issue: Issue
  board: Board
  teamId: string
  teamSlug: string
  currentUserId: string
}) {
  const isMember = useIsTeamMember(teamId, currentUserId)

  return (
    <PrRow
      issue={issue}
      board={board}
      teamSlug={teamSlug}
      isMember={isMember}
    />
  )
}

// ── Running / remote-start row ────────────────────────────────────────────────

function AgentRow({
  issue,
  board,
  teamId,
  users,
  isMember,
  steerEnabled,
  variant,
}: {
  issue: Issue
  board: Board
  teamId: string
  users: User[]
  isMember: boolean
  /** null while steer.config is still loading. */
  steerEnabled: boolean | null
  variant: CodingControlVariant
}) {
  const dock = useAgentDock()

  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) =>
          and(
            eq(s.issueId, issue.id),
            inArray(s.status, [`running`, `in_review`])
          )
        ),
    [issue.id]
  )
  // Staleness guard (EXP-153): heartbeat-dead rows render as absent.
  // Multi-window desktops can run several sessions on one issue; surface the
  // most recent (the badge counts them all).
  const now = useNow()
  const sessions = ((sessionRows ?? []) as CodingSession[]).filter(
    (s) => !isCodingSessionStale(s.updatedAt, now)
  )
  const latest = useMemo(() => {
    if (sessions.length === 0) return null
    return sessions.reduce((newest, row) =>
      new Date(row.startedAt) > new Date(newest.startedAt) ? row : newest
    )
  }, [sessions])

  if (latest) {
    const owner = users.find((u) => u.id === latest.userId)
    const codingBadge = (
      <SessionStatusBadge status={latest.status} count={sessions.length} />
    )
    const ownerLabel = (
      <span className="truncate text-xs text-muted-foreground">
        {displayUserName(owner, latest.userId)}
        {latest.deviceLabel ? ` · ${latest.deviceLabel}` : ``}
      </span>
    )

    if (variant === `sidebar`) {
      return (
        <PropertyGroup label="Agent">
          <div className="w-full space-y-2">
            <div className="flex min-w-0 items-center gap-2">
              {codingBadge}
              {ownerLabel}
            </div>
            {isMember && steerEnabled ? (
              <Button
                variant="outline"
                size="sm"
                className="w-full"
                onClick={() => dock?.openDock(latest.id)}
              >
                <MonitorPlay />
                Watch
              </Button>
            ) : isMember && steerEnabled === false ? (
              <p className="text-xs text-muted-foreground">
                Live steering is unavailable on this instance.
              </p>
            ) : null}
          </div>
        </PropertyGroup>
      )
    }

    return (
      <div className="flex min-w-0 items-center gap-2 border-t border-border px-4 py-3">
        {codingBadge}
        {ownerLabel}
        {isMember && steerEnabled ? (
          <Button
            variant="outline"
            size="sm"
            className="ml-auto shrink-0"
            onClick={() => dock?.openDock(latest.id)}
          >
            <MonitorPlay />
            Watch
          </Button>
        ) : isMember && steerEnabled === false ? (
          <span className="ml-auto text-xs text-muted-foreground">
            Live steering is unavailable on this instance.
          </span>
        ) : null}
      </div>
    )
  }

  // Not running: only members can remote-start, and only on a repo-backed
  // board with the relay enabled. Gate the desktop-presence fetch behind that
  // — RemoteStartRow (which owns useRemoteCodingStart) mounts ONLY here, so a
  // non-member / steer-off / repo-less / already-running issue view never fires
  // an ungated steer.myDevices round-trip.
  if (!isMember || !steerEnabled || !board.repositoryId) return null
  return <RemoteStartRow issue={issue} teamId={teamId} variant={variant} />
}

// The remote-start affordance — split out so its steer.myDevices fetch only
// runs when the start row can actually render (AgentRow gates the mount).
function RemoteStartRow({
  issue,
  teamId,
  variant,
}: {
  issue: Issue
  teamId: string
  variant: CodingControlVariant
}) {
  const remote = useRemoteCodingStart()
  const [dialogOpen, setDialogOpen] = useState(false)

  // Presence lookup still in flight — keep the section quiet.
  if (remote.devices === null) return null
  if (remote.devices.length === 0) {
    if (variant === `sidebar`) {
      return (
        <PropertyGroup label="Agent">
          <div className="flex items-start gap-1.5 text-xs text-muted-foreground">
            <MonitorOff className="mt-0.5 size-3.5 shrink-0" />
            <span>
              No desktop online — open the Exponential desktop app to run this
              issue there.
            </span>
          </div>
        </PropertyGroup>
      )
    }
    return (
      <div className="flex items-center gap-2 border-t border-border px-4 py-3 text-xs text-muted-foreground">
        <MonitorOff className="size-3.5 shrink-0" />
        No desktop online — open the Exponential desktop app to run this issue
        there.
      </div>
    )
  }

  const busy = remote.starting || remote.sentTo !== null
  const startButton = (
    <Button
      variant="outline"
      size="sm"
      className={variant === `sidebar` ? `w-full` : undefined}
      onClick={() => setDialogOpen(true)}
      disabled={busy}
    >
      {remote.starting ? <Loader2 className="animate-spin" /> : <MonitorUp />}
      Start coding
    </Button>
  )
  const dialog = (
    <StartCodingDialog
      open={dialogOpen}
      onOpenChange={setDialogOpen}
      devices={remote.devices}
      starting={remote.starting}
      teamId={teamId}
      initialIssueIds={[issue.id]}
      onStart={(device, options, issueIds) => {
        remote
          .start(device, options, issueIds)
          .then(() => setDialogOpen(false))
          .catch(() => {})
      }}
    />
  )

  if (variant === `sidebar`) {
    return (
      <PropertyGroup label="Agent">
        <div className="w-full space-y-2">
          {startButton}
          {dialog}
          {remote.sentTo && (
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Loader2 className="size-3 shrink-0 animate-spin" />
              Start sent to {remote.sentTo} — waiting for the desktop…
            </p>
          )}
        </div>
      </PropertyGroup>
    )
  }

  return (
    <div className="flex items-center gap-2 border-t border-border px-4 py-3">
      {startButton}
      {dialog}
      {remote.sentTo && (
        <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
          <Loader2 className="size-3 animate-spin" />
          Start sent to {remote.sentTo} — waiting for the desktop…
        </span>
      )}
    </div>
  )
}

// ── PR / pushed-branch row ────────────────────────────────────────────────────

function PrRow({
  issue,
  board,
  teamSlug,
  isMember,
}: {
  issue: Issue
  board: Board
  teamSlug: string
  isMember: boolean
}) {
  const hasPr = issue.prNumber != null
  // Tier-3 probe: a pushed branch with no PR yet, only while the issue is in a
  // coding-ish state (in_progress/in_review). One lookup per issue-id mount.
  const canProbe =
    !hasPr &&
    isMember &&
    Boolean(board.repositoryId) &&
    (issue.status === `in_progress` || issue.status === `in_review`)
  const [branchFileCount, setBranchFileCount] = useState<number | null>(null)

  useEffect(() => {
    if (!canProbe) {
      setBranchFileCount(null)
      return
    }
    let cancelled = false
    trpc.repositories.branchDiff
      .query({ issueId: issue.id })
      .then((res) => {
        if (!cancelled) setBranchFileCount(res?.files.length ?? 0)
      })
      .catch(() => {
        if (!cancelled) setBranchFileCount(null)
      })
    return () => {
      cancelled = true
    }
  }, [canProbe, issue.id])

  const rowClass = `flex min-w-0 items-center gap-2 border-t border-border px-4 py-3 text-sm hover:bg-muted/50`

  if (hasPr) {
    return (
      <Link
        to="/t/$teamSlug/reviews/$issueIdentifier"
        params={{ teamSlug, issueIdentifier: issue.identifier }}
        className={rowClass}
      >
        <GitPullRequest className="size-4 shrink-0 text-muted-foreground" />
        <PrStateBadge state={issue.prState} />
        <span className="shrink-0 font-mono">PR #{issue.prNumber}</span>
        {issue.branch && (
          <span className="hidden truncate font-mono text-xs text-muted-foreground md:inline">
            {issue.branch}
          </span>
        )}
        <ChevronRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
      </Link>
    )
  }

  if (canProbe && branchFileCount != null && branchFileCount > 0) {
    return (
      <Link
        to="/t/$teamSlug/reviews/$issueIdentifier"
        params={{ teamSlug, issueIdentifier: issue.identifier }}
        className={rowClass}
      >
        <GitBranch className="size-4 shrink-0 text-muted-foreground" />
        <span className="truncate">
          Branch <span className="font-mono">exp/{issue.identifier}</span>
          {` — no PR yet`}
        </span>
        <ChevronRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
      </Link>
    )
  }

  return null
}
