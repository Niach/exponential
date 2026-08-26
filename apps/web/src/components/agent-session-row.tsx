import { useEffect, useState, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { GitMerge, LoaderCircle } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { AgentSessionRow } from "@/hooks/use-agents-data"
import {
  sessionDisplayState,
  type SessionDisplayState,
} from "@/components/issue-coding-rows"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { GlassRow } from "@/components/ui/glass-rows"

// EXP-616: the trailing bare glyph that opens the linked issue — the iOS
// carded row's secondary destination, a cross-client concept never a raw
// lucide import.
const IssueDetailsIcon = conceptIcon(`ui-info`)

// The live coding-session row shared by the team Agents page and the Actions
// page (EXP-253) — extracted from routes/t/$teamSlug/agents/index.tsx so both
// surfaces render identical rows. Labeling is three-way: an action run
// (actionName snapshot set — survives the action's deletion) shows
// "Action" + the action name, an issueless batch run shows "Batch",
// everything else is the linked issue.

export function SectionLabel({
  label,
  count,
  trailing,
}: {
  label: string
  count: number
  /** Optional right-aligned control (e.g. the Actions "New action" button). */
  trailing?: ReactNode
}) {
  return (
    <div className="flex items-center gap-1.5 rounded-t-md border-b border-border/50 bg-zinc-500/10 px-3 py-1.5">
      <span className="text-sm font-medium">{label}</span>
      <span className="text-xs text-muted-foreground">{count}</span>
      {trailing && <div className="ml-auto">{trailing}</div>}
    </div>
  )
}

// Steady dot per parked display state (EXP-194/EXP-214): review green,
// done blue (both matching the issue-status palette), needs-input amber;
// running keeps the emerald ping.
const STATE_DOT: Record<Exclude<SessionDisplayState, `running`>, string> = {
  needs_input: `bg-amber-500`,
  review: `bg-emerald-500`,
  done: `bg-sky-500`,
}

const STATE_LABEL: Record<
  Exclude<SessionDisplayState, `running`>,
  { text: string; className: string }
> = {
  needs_input: { text: `Needs input`, className: `text-amber-400` },
  review: { text: `Ready for review`, className: `text-emerald-400` },
  done: { text: `Done`, className: `text-sky-400` },
}

export function RunningIndicator({
  state,
  paused = false,
}: {
  state: SessionDisplayState
  /** EXP-550: the host machine is offline — a steady grey dot, no ping. */
  paused?: boolean
}) {
  if (paused) {
    return (
      <span className="inline-flex size-2 rounded-full bg-muted-foreground/40" />
    )
  }
  if (state !== `running`) {
    return (
      <span
        className={`inline-flex size-2 rounded-full ${STATE_DOT[state]}`}
      />
    )
  }
  return (
    <span className="relative flex size-2">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

// Merge always closes (EXP-498): merges the PR, completes every linked
// issue, and ends the session server-side. closeSessions: true stays on the
// wire so an old (pre-498) server closes too. Spinner held until the
// Electric echo flips the issue's prState away from `open` (mirrors
// IssueMergeButton).
function SessionMergeButton({
  prState,
  prNumber,
  issueId,
}: {
  prState: string | null
  prNumber: number | null
  issueId: string
}) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [merging, setMerging] = useState(false)

  useEffect(() => {
    if (prState !== `open`) {
      setMerging(false)
      setConfirmOpen(false)
    }
  }, [prState])

  if (prState !== `open`) return null

  const merge = async () => {
    setMerging(true)
    try {
      // Failures surface via the global mutation-error toast.
      await trpc.issues.mergePr.mutate({ issueId, closeSessions: true })
      setConfirmOpen(false) // keep `merging` until the echo flips prState
    } catch {
      setMerging(false)
    }
  }

  return (
    <>
      <Button
        variant="outline"
        size="icon"
        className="size-8"
        disabled={merging}
        aria-label={merging ? `Merging…` : `Merge pull request`}
        title={merging ? `Merging…` : `Merge`}
        onClick={(e) => {
          e.stopPropagation()
          setConfirmOpen(true)
        }}
      >
        {merging ? <LoaderCircle className="animate-spin" /> : <GitMerge />}
      </Button>
      <Dialog
        open={confirmOpen}
        onOpenChange={(next) => {
          if (!merging) setConfirmOpen(next)
        }}
      >
        <DialogContent
          className="sm:max-w-sm"
          onClick={(e) => e.stopPropagation()}
        >
          <DialogHeader>
            <DialogTitle>Merge pull request?</DialogTitle>
            <DialogDescription>
              {`Merge PR #${prNumber ?? ``} into the default branch? Every issue linked to it completes, and its coding session closes.`}
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
              {merging ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <GitMerge />
              )}
              Merge
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

export function SessionRow({
  row,
  teamSlug,
  onOpen,
}: {
  row: AgentSessionRow
  teamSlug: string
  onOpen: () => void
}) {
  const { session, issue, board } = row
  const isAction = session.actionName != null
  const isBatch = !session.issueId
  // EXP-535: batch rows merge through their resolved PR's representative
  // issue (use-agents-data) — same button, same server call as issue rows.
  const prIssue = issue ?? row.batchPrIssue
  const displayState = sessionDisplayState(session, issue?.prState)
  // EXP-549/550: the host machine per the synced devices row — its RENAMED
  // label, and greyed-out "Paused" while it is offline (the agent is parked,
  // not gone; it resumes when the machine comes back).
  const { device, paused } = row

  // FEED-15: the native two-line row — dot | identifier + title, then the
  // parked-state label + "device · started …" byline | icon-only Merge and
  // the bare issue-details glyph — instead of the old four-column grid whose
  // inline state label collided with the buttons on phones. Every row is the
  // caller's own (EXP-312), so the byline names the machine, never the person.
  return (
    <GlassRow
      interactive
      className={paused ? `opacity-60` : undefined}
      onClick={onOpen}
      data-testid={`agent-session-${issue?.identifier ?? session.id}`}
      title={paused ? `${device.label ?? `The device`} is offline` : undefined}
    >
      <span className="flex w-3 shrink-0 items-center justify-center">
        <RunningIndicator state={displayState} paused={paused} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          {/* EXP-616: plain text — the trailing info glyph owns issue
              navigation now (iOS parity). */}
          <span className="shrink-0 font-mono text-xs text-muted-foreground">
            {isAction
              ? `Action`
              : issue && board
                ? issue.identifier
                : isBatch
                  ? `Batch`
                  : `—`}
          </span>
          <span className="truncate font-medium">
            {isAction
              ? session.actionName
              : isBatch
                ? `Batch session`
                : (issue?.title ?? `Issue syncing…`)}
          </span>
        </div>
        <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
          {paused ? (
            <span className="shrink-0 font-medium">Paused</span>
          ) : (
            displayState !== `running` && (
              <span
                className={`shrink-0 font-medium ${STATE_LABEL[displayState].className}`}
              >
                {STATE_LABEL[displayState].text}
              </span>
            )
          )}
          <span className="truncate">
            {`${device.label || session.deviceLabel || `Desktop`}${paused ? ` (offline)` : ``} · started ${relativeTime(session.startedAt)}`}
          </span>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {prIssue && (
          <SessionMergeButton
            prState={prIssue.prState}
            prNumber={prIssue.prNumber}
            issueId={prIssue.id}
          />
        )}
        {issue && board && (
          <Link
            to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
            params={{
              teamSlug,
              boardSlug: board.slug,
              issueIdentifier: issue.identifier,
            }}
            onClick={(e) => e.stopPropagation()}
            aria-label="Issue details"
            className="flex size-8 shrink-0 items-center justify-center rounded-full text-foreground/70 hover:text-foreground"
          >
            <IssueDetailsIcon className="size-4" />
          </Link>
        )}
      </div>
    </GlassRow>
  )
}
