import { useEffect, useMemo, useState } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
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
import type { CodingSession, Issue, Project, User } from "@/db/schema"
import {
  codingSessionCollection,
  workspaceMemberCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { displayUserName } from "@/lib/user-display"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { useRemoteCodingStart } from "@/hooks/use-remote-coding-start"
import { StartCodingDialog } from "@/components/start-coding-dialog"

// The coding section of the issue detail (EXP-106): a compact "coding now" /
// remote-start row that FOCUSES the global dock (never mounts the live viewer
// itself), plus a PR / pushed-branch row that links to the review-detail route.
// Repo presence + membership + relay availability gate it (the same signals the
// server enforces); everything degrades to nothing when they're absent.

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

interface IssueCodingRowsProps {
  issue: Issue
  project: Project
  workspaceId: string
  workspaceSlug: string
  currentUserId: string
  users: User[]
}

export function IssueCodingRows({
  issue,
  project,
  workspaceId,
  workspaceSlug,
  currentUserId,
  users,
}: IssueCodingRowsProps) {
  const config = useSteerConfig()

  // Steer + branch-probe affordances require workspace membership (the server
  // enforces this regardless; this only decides what renders).
  const { data: memberRows } = useLiveQuery(
    (query) =>
      query
        .from({ m: workspaceMemberCollection })
        .where(({ m }) =>
          and(eq(m.workspaceId, workspaceId), eq(m.userId, currentUserId))
        ),
    [workspaceId, currentUserId]
  )
  const isMember = (memberRows?.length ?? 0) > 0

  return (
    <>
      <AgentRow
        issue={issue}
        project={project}
        workspaceId={workspaceId}
        users={users}
        isMember={isMember}
        steerEnabled={config?.enabled ?? null}
      />
      <PrRow
        issue={issue}
        project={project}
        workspaceSlug={workspaceSlug}
        isMember={isMember}
      />
    </>
  )
}

// ── Running / remote-start row ────────────────────────────────────────────────

function AgentRow({
  issue,
  project,
  workspaceId,
  users,
  isMember,
  steerEnabled,
}: {
  issue: Issue
  project: Project
  workspaceId: string
  users: User[]
  isMember: boolean
  /** null while steer.config is still loading. */
  steerEnabled: boolean | null
}) {
  const dock = useAgentDock()

  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) => and(eq(s.issueId, issue.id), eq(s.status, `running`))),
    [issue.id]
  )
  // Multi-window desktops can run several sessions on one issue; surface the
  // most recent (the badge counts them all).
  const sessions = (sessionRows ?? []) as CodingSession[]
  const latest = useMemo(() => {
    if (sessions.length === 0) return null
    return sessions.reduce((newest, row) =>
      new Date(row.startedAt) > new Date(newest.startedAt) ? row : newest
    )
  }, [sessions])

  if (latest) {
    const owner = users.find((u) => u.id === latest.userId)
    return (
      <div className="flex min-w-0 items-center gap-2 border-t border-border px-4 py-3">
        <Badge
          variant="outline"
          className="gap-1.5 border-emerald-500/40 text-emerald-400"
        >
          <RunningPing />
          Coding now
          {sessions.length > 1 ? ` (·${sessions.length})` : ``}
        </Badge>
        <span className="truncate text-xs text-muted-foreground">
          {displayUserName(owner, latest.userId)}
          {latest.deviceLabel ? ` · ${latest.deviceLabel}` : ``}
        </span>
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
  // project with the relay enabled. Gate the desktop-presence fetch behind that
  // — RemoteStartRow (which owns useRemoteCodingStart) mounts ONLY here, so a
  // non-member / steer-off / repo-less / already-running issue view never fires
  // an ungated steer.myDevices round-trip.
  if (!isMember || !steerEnabled || !project.repositoryId) return null
  return <RemoteStartRow issue={issue} workspaceId={workspaceId} />
}

// The remote-start affordance — split out so its steer.myDevices fetch only
// runs when the start row can actually render (AgentRow gates the mount).
function RemoteStartRow({
  issue,
  workspaceId,
}: {
  issue: Issue
  workspaceId: string
}) {
  const remote = useRemoteCodingStart()
  const [dialogOpen, setDialogOpen] = useState(false)

  // Presence lookup still in flight — keep the section quiet.
  if (remote.devices === null) return null
  if (remote.devices.length === 0) {
    return (
      <div className="flex items-center gap-2 border-t border-border px-4 py-3 text-xs text-muted-foreground">
        <MonitorOff className="size-3.5 shrink-0" />
        No desktop online — open the Exponential desktop app to run this issue
        there.
      </div>
    )
  }

  const busy = remote.starting || remote.sentTo !== null
  return (
    <div className="flex items-center gap-2 border-t border-border px-4 py-3">
      <Button
        variant="outline"
        size="sm"
        onClick={() => setDialogOpen(true)}
        disabled={busy}
      >
        {remote.starting ? <Loader2 className="animate-spin" /> : <MonitorUp />}
        Start coding
      </Button>
      <StartCodingDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        devices={remote.devices}
        starting={remote.starting}
        workspaceId={workspaceId}
        initialIssueIds={[issue.id]}
        onStart={(device, options, issueIds) => {
          remote
            .start(device, options, issueIds)
            .then(() => setDialogOpen(false))
            .catch(() => {})
        }}
      />
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
  project,
  workspaceSlug,
  isMember,
}: {
  issue: Issue
  project: Project
  workspaceSlug: string
  isMember: boolean
}) {
  const hasPr = issue.prNumber != null
  // Tier-3 probe: a pushed branch with no PR yet, only while the issue is in a
  // coding-ish state (in_progress/in_review). One lookup per issue-id mount.
  const canProbe =
    !hasPr &&
    isMember &&
    Boolean(project.repositoryId) &&
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

  const rowClass =
    `flex min-w-0 items-center gap-2 border-t border-border px-4 py-3 text-sm hover:bg-muted/50`

  if (hasPr) {
    return (
      <Link
        to="/t/$workspaceSlug/reviews/$issueIdentifier"
        params={{ workspaceSlug, issueIdentifier: issue.identifier }}
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
        to="/t/$workspaceSlug/reviews/$issueIdentifier"
        params={{ workspaceSlug, issueIdentifier: issue.identifier }}
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
