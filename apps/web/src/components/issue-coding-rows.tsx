import { useEffect, useMemo, useState, type ReactNode } from "react"
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
import { useSessionDevice } from "@/hooks/use-session-device"
import { sessionIsPaused } from "@/lib/session-device"
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
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { GlassRow } from "@/components/ui/glass-rows"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"

// EXP-317: the "no desktop online" hint draws the same glyph here and in
// the native apps (`ui-device-offline`).
const UiDeviceOfflineIcon = conceptIcon(`ui-device-offline`)
// The phone bar's start button — the same glyph the Actions surfaces run with.
const ActionRunIcon = conceptIcon(`action-run`)

// EXP-568: the floating mobile bar's 52px circles (issue-detail-mobile-bar.tsx
// owns the bar itself; the coding circle's gating lives here).
const FAB_CIRCLE_CLASS = `pointer-events-auto flex size-[52px] shrink-0 items-center justify-center rounded-full border border-glass-stroke-card bg-popover/85 shadow-lg shadow-black/40 backdrop-blur-xl`

// EXP-616: the coding / PR rows are glass CARDS now, not full-bleed
// `border-t` divider rows. Both exported pieces mount as independent siblings
// of the issue-detail main column (issue-detail-view.tsx owns no wrapper), so
// each one carries its own stack + gutter — the same `px-5 py-2` gutter
// issue-files-section.tsx uses, so the cards line up down the column.
function CodingRowStack({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-2 px-5 py-2">{children}</div>
}

// The PR row is a <Link>, so it can't reuse the <GlassRow> div — same recipe,
// interactive arm included.
const PR_ROW_CLASS = `flex min-w-0 items-center gap-2 rounded-md border border-glass-stroke bg-glass-row p-3 text-sm transition-colors duration-fast hover:bg-glass-active/50`

// The coding affordances of the issue detail (EXP-106): a compact "coding now"
// / remote-start control that FOCUSES the global dock (never mounts the live
// viewer itself), plus a PR / pushed-branch row that links to the review-detail
// route. Repo presence + membership + relay availability gate them (the same
// signals the server enforces); everything degrades to nothing when absent.
// EXP-184 split them: IssueCodingControl renders as the full-width main-column
// row (variant='row', both viewports since EXP-568) or as the phone bottom
// bar's circle (variant='fab'); IssuePrRow always stays a main-column row.
// EXP-616 added variant='start': the bare "Start coding" capsule the issue
// detail hangs off its properties card, which the 'row' variant therefore no
// longer draws.

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

// The display-state derivation lives in a plain lib module so it can be
// unit-tested without dragging the component graph in (EXP-531); re-exported
// here for the existing importers.
import {
  sessionDisplayState,
  type SessionDisplayState,
} from "@/lib/coding-session-display"

export { sessionDisplayState, type SessionDisplayState }

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
  paused = false,
}: {
  session: Pick<CodingSession, `status` | `needsInput`>
  prState: string | null | undefined
  count?: number
  /** EXP-550: the host machine is offline — the agent is parked, not gone.
   * Renders a grey "Paused" instead of the live/needs-input badge. */
  paused?: boolean
}) {
  const state = sessionDisplayState(session, prState)
  if (paused) {
    return (
      <Badge
        variant="outline"
        className="gap-1.5 border-border text-muted-foreground"
      >
        <StateDot className="bg-muted-foreground/40" />
        Paused
        {count > 1 ? ` (·${count})` : ``}
      </Badge>
    )
  }
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

/** `row` = the main-column glass rows (running session / merge / "no desktop
 * online"), `fab` = the phone bar's circle, `start` = the bare "Start coding"
 * capsule the issue detail's properties card hosts (EXP-616, desktop parity
 * with the IDE — it renders NOTHING in the states the `row` variant covers, so
 * the two mounts never draw the same affordance twice). */
export type CodingControlVariant = `row` | `fab` | `start`

// Sidebar merge affordance (EXP-268): full-width Merge button + confirm
// dialog for an issue whose linked PR is open. Mirrors the reviews pages'
// semantics — `issues.mergePr`, spinner held until the Electric echo flips
// `prState` away from `open`. Merge always closes the live coding sessions
// (EXP-498).
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
        className="shrink-0"
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
        <DialogContent mobile="alert" className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Merge pull request?</DialogTitle>
            <DialogDescription>
              {`Merge PR #${issue.prNumber ?? ``} into the default branch? Every issue linked to it completes, and any live coding session for it closes.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              onClick={() => setConfirmOpen(false)}
              disabled={merging}
            />
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

// Membership gate shared by both exported pieces and the bulk bar's
// "Start coding" button (the server enforces it regardless; this only decides
// what renders).
export function useIsTeamMember(teamId: string, currentUserId: string) {
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

/** The "coding now" / remote-start control — main-column row or phone circle. */
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
  // EXP-312: live sessions are owner-only — the Watch affordance targets the
  // caller's own most-recent session (teammates see just badge + byline).
  const ownLatest = useMemo(() => {
    const own = sessions.filter((s) => s.userId === currentUserId)
    if (own.length === 0) return null
    return own.reduce((newest, row) =>
      new Date(row.startedAt) > new Date(newest.startedAt) ? row : newest
    )
  }, [sessions, currentUserId])
  // EXP-549/550: the latest session's host machine per the synced devices
  // row — renamed label, and "Paused" while that machine is offline.
  const latestDevice = useSessionDevice(latest)

  if (latest) {
    // A live session replaces the start affordance — the properties card's
    // capsule steps aside for the running row below.
    if (variant === `start`) return null
    const owner = users.find((u) => u.id === latest.userId)
    const paused = sessionIsPaused(
      sessionDisplayState(latest, issue.prState),
      latestDevice
    )

    // EXP-568 phone bar: one 52px circle, no words. Own live session → tap to
    // open the dock; someone else's → a static badge circle that says "busy,
    // not yours" (EXP-312 keeps live sessions owner-only).
    if (variant === `fab`) {
      if (ownLatest && steerEnabled) {
        return (
          <button
            type="button"
            aria-label="Open coding session"
            onClick={() => dock?.openDock(ownLatest.id)}
            className={cn(FAB_CIRCLE_CLASS, `text-emerald-400`)}
          >
            {paused ? (
              <StateDot className="bg-muted-foreground/40" />
            ) : (
              <RunningPing />
            )}
          </button>
        )
      }
      return (
        <div
          aria-label="Coding session running"
          className={cn(FAB_CIRCLE_CLASS, `text-muted-foreground`)}
        >
          {paused ? (
            <StateDot className="bg-muted-foreground/40" />
          ) : (
            <RunningPing />
          )}
        </div>
      )
    }

    const codingBadge = (
      <SessionStatusBadge
        session={latest}
        prState={issue.prState}
        count={sessions.length}
        paused={paused}
      />
    )
    const ownerLabel = (
      <span
        className="truncate text-xs text-muted-foreground"
        title={paused ? `${latestDevice.label ?? `The device`} is offline` : undefined}
      >
        {displayUserName(owner, latest.userId)}
        {latestDevice.label ? ` · ${latestDevice.label}` : ``}
        {paused ? ` (offline)` : ``}
      </span>
    )

    return (
      <CodingRowStack>
        <GlassRow className="min-w-0 flex-wrap gap-2">
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
          {/* EXP-568: the sidebar is gone, so its Merge button lives here. */}
          {isMember && issue.prState === `open` && (
            <IssueMergeButton issue={issue} />
          )}
        </GlassRow>
      </CodingRowStack>
    )
  }

  // Not running: only members can remote-start, and only on a repo-backed
  // board with the relay enabled. Gate the device wiring behind that —
  // RemoteStartRow (which owns useRemoteStart over the synced devices shape)
  // mounts ONLY here, so a non-member / steer-off / repo-less /
  // already-running issue view never wires it up.
  if (!isMember || !steerEnabled || !board.repositoryId) {
    // An open PR still deserves its Merge button (EXP-268) even when remote
    // start can't render (steer off / repo-less board) — a main-column row,
    // never the properties card's capsule.
    if (variant === `start`) return null
    if (isMember && variant === `row` && issue.prState === `open`) {
      return (
        <CodingRowStack>
          <GlassRow className="gap-2">
            <IssueMergeButton issue={issue} />
          </GlassRow>
        </CodingRowStack>
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

// The remote-start affordance — split out so its device wiring only
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
    // Nothing to start on: the phone bar simply drops the circle rather than
    // spending one of its three slots on an explanation, and the properties
    // card drops its capsule — the row below carries the explanation.
    if (variant === `fab` || variant === `start`) return null
    return (
      <CodingRowStack>
        <GlassRow className="flex-wrap gap-2 text-xs text-muted-foreground">
          <UiDeviceOfflineIcon className="size-3.5 shrink-0" />
          No desktop online. Open the Exponential desktop app to run this issue
          there.
          {issue.prState === `open` && <IssueMergeButton issue={issue} />}
        </GlassRow>
      </CodingRowStack>
    )
  }

  const busy = remote.starting || remote.sentTo !== null
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

  if (variant === `fab`) {
    return (
      <>
        <button
          type="button"
          aria-label="Start coding"
          disabled={busy}
          onClick={() => setDialogOpen(true)}
          className={cn(FAB_CIRCLE_CLASS, `text-foreground disabled:opacity-60`)}
        >
          {busy ? (
            <LoaderCircle className="size-5 animate-spin" />
          ) : (
            <ActionRunIcon className="size-5" />
          )}
        </button>
        {dialog}
      </>
    )
  }

  // EXP-616: the start affordance is a capsule INSIDE the issue's properties
  // card (desktop parity with the IDE) — no standalone row of its own.
  if (variant === `start`) {
    return (
      <div className="flex min-w-0 items-center gap-2">
        {remote.sentTo && (
          <span className="hidden min-w-0 items-center gap-1.5 truncate text-xs text-muted-foreground lg:inline-flex">
            <LoaderCircle className="size-3 shrink-0 animate-spin" />
            Start sent to {remote.sentTo}. Waiting for the desktop…
          </span>
        )}
        <Button
          variant="glass"
          size="xs"
          onClick={() => setDialogOpen(true)}
          disabled={busy}
        >
          {busy ? <LoaderCircle className="animate-spin" /> : <MonitorUp />}
          Start coding
        </Button>
        {dialog}
      </div>
    )
  }

  // The main column keeps only what the capsule can't carry: an open PR's
  // Merge button (EXP-268). With nothing to show it draws no empty card.
  if (issue.prState !== `open`) return null
  return (
    <CodingRowStack>
      <GlassRow className="flex-wrap gap-2">
        <IssueMergeButton issue={issue} />
      </GlassRow>
    </CodingRowStack>
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

  if (hasPr) {
    return (
      <CodingRowStack>
        <Link
          to="/t/$teamSlug/reviews/$issueIdentifier"
          params={{ teamSlug, issueIdentifier: issue.identifier }}
          className={PR_ROW_CLASS}
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
      </CodingRowStack>
    )
  }

  if (canProbe && branchFileCount != null && branchFileCount > 0) {
    return (
      <CodingRowStack>
        <Link
          to="/t/$teamSlug/reviews/$issueIdentifier"
          params={{ teamSlug, issueIdentifier: issue.identifier }}
          className={PR_ROW_CLASS}
        >
          <GitBranch className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate">
            Branch <span className="font-mono">exp/{issue.identifier}</span>
            {` · no PR yet`}
          </span>
          <ChevronRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
        </Link>
      </CodingRowStack>
    )
  }

  return null
}
