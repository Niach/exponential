import { useEffect, useMemo, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import {
  ChevronRight,
  GitBranch,
  GitMerge,
  GitPullRequest,
  LoaderCircle,
  MonitorPlay,
  MonitorUp,
} from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { PropertyGroup } from "@/components/issue-properties-panel"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"

// EXP-317: the "no desktop online" hint draws the same glyph here and in
// the native apps (`ui-device-offline`).
const UiDeviceOfflineIcon = conceptIcon(`ui-device-offline`)

// The coding affordances of the issue detail (EXP-106): a compact "coding now"
// / remote-start control that FOCUSES the global dock (never mounts the live
// viewer itself), plus a PR / pushed-branch row that links to the review-detail
// route. Repo presence + membership + relay availability gate them (the same
// signals the server enforces); everything degrades to nothing when absent.
// EXP-184 split them: IssueCodingControl renders as an unlabeled sidebar
// property group on desktop (variant='sidebar') or the classic full-width row
// on mobile (variant='row'); IssuePrRow always stays a main-column row.

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

/** How a live session should render (EXP-214) — the session status alone is
 * not the whole story: `in_review` splits on the linked issue's PR outcome
 * (merged → the run is done, green review otherwise, matching the issue-status
 * palette), and a desktop-reported pending picker (plan approval /
 * AskUserQuestion) overrides everything visible as "needs input". The real
 * `merged` status (EXP-358 — the server parks sessions there on PR merge
 * instead of killing them) wins outright; the in_review+prState fallback
 * stays for rows written by pre-358 servers. */
export type SessionDisplayState =
  | `needs_input`
  | `running`
  | `review`
  | `merged`
  | `done`

export function sessionDisplayState(
  session: Pick<CodingSession, `status` | `needsInput`>,
  prState: string | null | undefined
): SessionDisplayState {
  if (session.status === `merged`) return `merged`
  const merged = prState === `merged`
  if (session.needsInput && !merged) return `needs_input`
  if (session.status === `in_review`) return merged ? `done` : `review`
  return `running`
}

/** Static counterpart of RunningPing for the parked states (EXP-194/EXP-214):
 * review green (matches the in_review issue status), done blue (matches the
 * done issue status), needs-input amber. */
function StateDot({ className }: { className: string }) {
  return <span className={cn(`inline-flex size-2 rounded-full`, className)} />
}

const SESSION_STATE_BADGE: Record<
  Exclude<SessionDisplayState, `running`>,
  { label: string; badge: string; dot: string }
> = {
  needs_input: {
    label: `Needs input`,
    badge: `border-amber-500/40 text-amber-400`,
    dot: `bg-amber-500`,
  },
  review: {
    label: `Ready for review`,
    badge: `border-emerald-500/40 text-emerald-400`,
    dot: `bg-emerald-500`,
  },
  merged: {
    label: `Merged`,
    badge: `border-sky-500/40 text-sky-400`,
    dot: `bg-sky-500`,
  },
  done: {
    label: `Done`,
    badge: `border-sky-500/40 text-sky-400`,
    dot: `bg-sky-500`,
  },
}

/** Live-session badge — "Coding now" / "Needs input" / "Ready for review" /
 * "Done". Shared by the issue detail row and the Agents page. */
export function SessionStatusBadge({
  session,
  prState,
  count = 1,
}: {
  session: Pick<CodingSession, `status` | `needsInput`>
  prState: string | null | undefined
  count?: number
}) {
  const state = sessionDisplayState(session, prState)
  if (state === `running`) {
    return (
      <Badge
        variant="outline"
        className="gap-1.5 border-emerald-500/40 text-emerald-400"
      >
        <RunningPing />
        Coding now
        {count > 1 ? ` (·${count})` : ``}
      </Badge>
    )
  }
  const style = SESSION_STATE_BADGE[state]
  return (
    <Badge variant="outline" className={cn(`gap-1.5`, style.badge)}>
      <StateDot className={style.dot} />
      {style.label}
      {count > 1 ? ` (·${count})` : ``}
    </Badge>
  )
}

export type CodingControlVariant = `row` | `sidebar`

// Sidebar merge affordance (EXP-268): full-width Merge button + confirm
// dialog for an issue whose linked PR is open. Mirrors the reviews pages'
// semantics — `issues.mergePr`, spinner held until the Electric echo flips
// `prState` away from `open`. Merge-only (EXP-358): the live session parks
// in `merged` and stays open — closing it is the session rows' "Merge and
// close" affordance.
function IssueMergeButton({ issue }: { issue: Issue }) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [merging, setMerging] = useState(false)

  // The live issue prop flips via Electric once the merge lands — release the
  // held spinner (and any stale confirm) then.
  useEffect(() => {
    if (issue.prState !== `open`) {
      setMerging(false)
      setConfirmOpen(false)
    }
  }, [issue.prState])

  const merge = async () => {
    setMerging(true)
    try {
      // Failures surface via the global mutation-error toast.
      await trpc.issues.mergePr.mutate({ issueId: issue.id })
      setConfirmOpen(false) // keep `merging` until the echo flips prState
    } catch {
      setMerging(false)
    }
  }

  return (
    <>
      <Button
        variant="outline"
        size="sm"
        className="w-full"
        onClick={() => setConfirmOpen(true)}
        disabled={merging}
      >
        {merging ? <LoaderCircle className="animate-spin" /> : <GitMerge />}
        {merging ? `Merging…` : `Merge PR`}
      </Button>
      <Dialog
        open={confirmOpen}
        onOpenChange={(next) => {
          if (!merging) setConfirmOpen(next)
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Merge pull request?</DialogTitle>
            <DialogDescription>
              {`Merge PR #${issue.prNumber ?? ``} into the default branch? Every issue linked to it completes.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setConfirmOpen(false)}
              disabled={merging}
            >
              Cancel
            </Button>
            <Button onClick={merge} disabled={merging}>
              {merging ? <LoaderCircle className="animate-spin" /> : <GitMerge />}
              Merge
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

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
      currentUserId={currentUserId}
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
  currentUserId,
  users,
  isMember,
  steerEnabled,
  variant,
}: {
  issue: Issue
  board: Board
  teamId: string
  currentUserId: string
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
            inArray(s.status, [`running`, `in_review`, `merged`])
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
  // EXP-312: live sessions are owner-only — the Watch affordance targets the
  // caller's own most-recent session (teammates see just badge + byline).
  const ownLatest = useMemo(() => {
    const own = sessions.filter((s) => s.userId === currentUserId)
    if (own.length === 0) return null
    return own.reduce((newest, row) =>
      new Date(row.startedAt) > new Date(newest.startedAt) ? row : newest
    )
  }, [sessions, currentUserId])

  if (latest) {
    const owner = users.find((u) => u.id === latest.userId)
    const codingBadge = (
      <SessionStatusBadge
        session={latest}
        prState={issue.prState}
        count={sessions.length}
      />
    )
    const ownerLabel = (
      <span className="truncate text-xs text-muted-foreground">
        {displayUserName(owner, latest.userId)}
        {latest.deviceLabel ? ` · ${latest.deviceLabel}` : ``}
      </span>
    )

    if (variant === `sidebar`) {
      return (
        <PropertyGroup>
          <div className="w-full space-y-2">
            <div className="flex min-w-0 items-center gap-2">
              {codingBadge}
              {ownerLabel}
            </div>
            {ownLatest && steerEnabled ? (
              <Button
                variant="outline"
                size="sm"
                className="w-full"
                onClick={() => dock?.openDock(ownLatest.id)}
              >
                <MonitorPlay />
                Watch
              </Button>
            ) : ownLatest && steerEnabled === false ? (
              <p className="text-xs text-muted-foreground">
                Live steering is unavailable on this instance.
              </p>
            ) : null}
            {isMember && issue.prState === `open` && (
              <IssueMergeButton issue={issue} />
            )}
          </div>
        </PropertyGroup>
      )
    }

    return (
      <div className="flex min-w-0 items-center gap-2 border-t border-border px-4 py-3">
        {codingBadge}
        {ownerLabel}
        {ownLatest && steerEnabled ? (
          <Button
            variant="outline"
            size="sm"
            className="ml-auto shrink-0"
            onClick={() => dock?.openDock(ownLatest.id)}
          >
            <MonitorPlay />
            Watch
          </Button>
        ) : ownLatest && steerEnabled === false ? (
          <span className="ml-auto text-xs text-muted-foreground">
            Live steering is unavailable on this instance.
          </span>
        ) : null}
      </div>
    )
  }

  // Not running: only members can remote-start, and only on a repo-backed
  // board with the relay enabled. Gate the desktop-presence fetch behind that
  // — RemoteStartRow (which owns useRemoteStart) mounts ONLY here, so a
  // non-member / steer-off / repo-less / already-running issue view never fires
  // an ungated steer.myDevices round-trip.
  if (!isMember || !steerEnabled || !board.repositoryId) {
    // An open PR still deserves its sidebar Merge button (EXP-268) even when
    // remote start can't render (steer off / repo-less board).
    if (isMember && variant === `sidebar` && issue.prState === `open`) {
      return (
        <PropertyGroup>
          <div className="w-full space-y-2">
            <IssueMergeButton issue={issue} />
          </div>
        </PropertyGroup>
      )
    }
    return null
  }
  return (
    <RemoteStartRow
      issue={issue}
      teamId={teamId}
      currentUserId={currentUserId}
      variant={variant}
    />
  )
}

// The remote-start affordance — split out so its steer.myDevices fetch only
// runs when the start row can actually render (AgentRow gates the mount).
function RemoteStartRow({
  issue,
  teamId,
  currentUserId,
  variant,
}: {
  issue: Issue
  teamId: string
  currentUserId: string
  variant: CodingControlVariant
}) {
  const remote = useRemoteStart({ currentUserId, teamId })
  const [dialogOpen, setDialogOpen] = useState(false)

  // Presence lookup still in flight — keep the section quiet.
  if (remote.devices === null) return null
  if (remote.devices.length === 0) {
    if (variant === `sidebar`) {
      return (
        <PropertyGroup>
          <div className="w-full space-y-2">
            <div className="flex items-start gap-1.5 text-xs text-muted-foreground">
              <UiDeviceOfflineIcon className="mt-0.5 size-3.5 shrink-0" />
              <span>
                No desktop online. Open the Exponential desktop app to run
                this issue there.
              </span>
            </div>
            {issue.prState === `open` && <IssueMergeButton issue={issue} />}
          </div>
        </PropertyGroup>
      )
    }
    return (
      <div className="flex items-center gap-2 border-t border-border px-4 py-3 text-xs text-muted-foreground">
        <UiDeviceOfflineIcon className="size-3.5 shrink-0" />
        No desktop online. Open the Exponential desktop app to run this issue
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
      {remote.starting ? <LoaderCircle className="animate-spin" /> : <MonitorUp />}
      Start coding
    </Button>
  )
  const dialog = (
    <LaunchDialog
      open={dialogOpen}
      onOpenChange={setDialogOpen}
      devices={remote.devices}
      starting={remote.starting}
      teamId={teamId}
      initialIssueIds={[issue.id]}
      onStartIssues={(device, options, issueIds) => {
        remote
          .startIssues(device, options, issueIds)
          .then(() => setDialogOpen(false))
          .catch(() => {})
      }}
      onRunAction={(device, action, options, inputs) => {
        remote
          .runAction(device, action, options, inputs)
          .then(() => setDialogOpen(false))
          .catch(() => {})
      }}
    />
  )

  if (variant === `sidebar`) {
    return (
      <PropertyGroup>
        <div className="w-full space-y-2">
          {startButton}
          {issue.prState === `open` && <IssueMergeButton issue={issue} />}
          {dialog}
          {remote.sentTo && (
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <LoaderCircle className="size-3 shrink-0 animate-spin" />
              Start sent to {remote.sentTo}. Waiting for the desktop…
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
          <LoaderCircle className="size-3 animate-spin" />
          Start sent to {remote.sentTo}. Waiting for the desktop…
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
          {` · no PR yet`}
        </span>
        <ChevronRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
      </Link>
    )
  }

  return null
}
