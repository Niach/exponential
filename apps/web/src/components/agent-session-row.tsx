import { useState, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { ChevronDown, LoaderCircle } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { AgentSessionRow } from "@/hooks/use-agents-data"
import type { CodingSession, SyncedAction } from "@/db/schema"
import {
  sessionDisplayState,
  type SessionDisplayState,
} from "@/components/issue-coding-rows"
import { relativeTime } from "@/components/comment-rows/format"
import { actionCollection } from "@/lib/collections"
import { getActionIcon } from "@/lib/board-icons"
import { trpc } from "@/lib/trpc-client"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { SessionMergeButton } from "@/components/session-merge-button"
import { GlassRow } from "@/components/ui/glass-rows"

// EXP-530: the automation glyph is a cross-client concept — the fallback when
// an automation run's action row has not synced (or was deleted).
const ActionAutomationIcon = conceptIcon(`action-automation`)

// The live coding-session row shared by the team Devices page and the
// Automations runs list (EXP-253/EXP-686) — extracted from the old Agents
// route so both surfaces render identical rows. Labeling is three-way: an
// action run
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

/** The action a run belongs to, off the synced (body-less) `actions` shape —
 * it carries the glyph the row's trailing button draws. An automation run
 * carries its automation's `action_id` too, so ONE lookup serves both.
 * Issue/chat/batch rows have no `actionId` and run no query at all. */
function useRunAction(actionId: string | null): SyncedAction | undefined {
  const { data } = useLiveQuery(
    (query) =>
      actionId
        ? query
            .from({ actions: actionCollection })
            .where(({ actions }) => eq(actions.id, actionId))
        : undefined,
    [actionId]
  )
  return ((data ?? []) as SyncedAction[])[0]
}

export function SessionRow({
  row,
  teamSlug,
  isOwner,
  currentUserId,
  steerEnabled = false,
  onOpen,
}: {
  row: AgentSessionRow
  teamSlug: string
  /** EXP-694: the caller's team role, from `useTeamPermissions` — it decides
   * WHICH editor the trailing button opens (a row must not resolve it itself:
   * the hook fetches the team's billing plan, once per row). */
  isOwner: boolean
  /** EXP-706: resolved ONCE by the caller (same reason as `isOwner`) — they
   * let a conflicted Merge swap itself for the "Fix conflicts" run. */
  currentUserId?: string
  steerEnabled?: boolean
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
  // EXP-694: the trailing button is the row's SECOND destination, one shape
  // per kind — the issue's identifier as a glass pill, an action/automation
  // run's own glyph as a glass circle opening its editor, and nothing at all
  // for a chat or batch run (there is nothing behind it to open).
  const runAction = useRunAction(session.actionId)
  // An automation run edits the AUTOMATION only for a team owner (that form
  // is a write surface with no read-only mode); every other member lands in
  // the action editor, which opens read-only by itself — iOS `editTarget`,
  // Android `onOpenAction`. The label follows the DESTINATION, never the kind.
  const editsAutomation = Boolean(session.automationId) && isOwner
  const RunIcon = runAction
    ? getActionIcon(runAction)
    : editsAutomation
      ? ActionAutomationIcon
      : getActionIcon({})

  // FEED-15: the native two-line row — dot | identifier + title, then the
  // parked-state label + "device · started …" byline | icon-only Merge and
  // the kind's own trailing button — instead of the old four-column grid whose
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
          {/* EXP-616: plain text — the trailing button owns issue
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
            branch={prIssue.branch}
            teamId={prIssue.teamId}
            currentUserId={currentUserId}
            steerEnabled={steerEnabled}
          />
        )}
        {issue && board ? (
          <Button asChild variant="glass" size="xs" className="font-mono">
            <Link
              to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
              params={{
                teamSlug,
                boardSlug: board.slug,
                issueIdentifier: issue.identifier,
              }}
              onClick={(e) => e.stopPropagation()}
              aria-label={`Open ${issue.identifier}`}
              title={issue.identifier}
            >
              {issue.identifier}
            </Link>
          </Button>
        ) : editsAutomation && session.automationId ? (
          <Button asChild variant="glass" size="icon" className="size-8">
            <Link
              to="/t/$teamSlug/actions"
              params={{ teamSlug }}
              search={{
                tab: `automations`,
                editAutomation: session.automationId,
              }}
              onClick={(e) => e.stopPropagation()}
              aria-label="Edit automation"
              title="Edit automation"
            >
              <RunIcon className="size-4" />
            </Link>
          </Button>
        ) : session.actionId ? (
          <Button asChild variant="glass" size="icon" className="size-8">
            <Link
              to="/t/$teamSlug/actions"
              params={{ teamSlug }}
              search={{ editAction: session.actionId }}
              onClick={(e) => e.stopPropagation()}
              aria-label="Edit action"
              title="Edit action"
            >
              <RunIcon className="size-4" />
            </Link>
          </Button>
        ) : null}
      </div>
    </GlassRow>
  )
}

// ── Ended runs (EXP-637) ─────────────────────────────────────────────────────
// A run that closed itself through `exponential_sessions_end` carries the
// agent's own account of it: a one-paragraph summary (EXP-686 dropped the
// self-reported outcome — the summary IS the report). Decision 5: it is NEVER
// inline. Collapsed shows the title and the time; expanding reveals the
// summary, rendered as the markdown the agent wrote, and Resume. This is the
// Automations tab's "Recent automated runs" row (EXP-676 dropped the Agents
// page's "Recent runs" list — only automated runs are listed now, so every row
// is an action run and no "Action" kind label is drawn), mirrored on desktop,
// iOS and Android.

const ResumeIcon = conceptIcon(`run-resume`)

/** What an ended-run row needs — the Automations tab builds both fields off
 * the synced session + device rows. */
export interface EndedRunRow {
  session: CodingSession
  /** EXP-637: the run's machine is online and advertises `resume-run`. */
  canResume: boolean
}

export function EndedSessionRow({
  row,
  /** The title line — the action that fired (the row's `actionName`
   * snapshot, or the live action's name when the snapshot is missing). */
  title,
}: {
  row: EndedRunRow
  title: string
}) {
  const { session, canResume } = row
  const [expanded, setExpanded] = useState(false)
  const [resuming, setResuming] = useState(false)

  // The resumed run arrives as a NEW row over Electric; the button only has
  // to send the command, so it settles as soon as the relay accepted it.
  const resume = async () => {
    if (!session.deviceId) return
    setResuming(true)
    try {
      await trpc.steer.startSession.mutate({
        resumeSessionId: session.id,
        deviceId: session.deviceId,
      })
    } catch (e) {
      toast.error(e instanceof Error ? e.message : `Could not resume that run`)
    } finally {
      setResuming(false)
    }
  }

  return (
    <GlassRow
      interactive
      className="flex-col items-stretch gap-2"
      onClick={() => setExpanded((open) => !open)}
      data-testid={`ended-session-${session.id}`}
    >
      <div className="flex min-w-0 items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center text-sm">
            <span className="truncate font-medium">{title}</span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2 text-xs">
          <span className="text-muted-foreground">
            {relativeTime(session.endedAt ?? session.startedAt)}
          </span>
          <ChevronDown
            className={`size-4 text-muted-foreground transition-transform duration-fast ${expanded ? `rotate-180` : ``}`}
            aria-hidden
          />
        </div>
      </div>
      {expanded && (
        <div
          className="flex flex-col gap-2 border-t border-glass-stroke pt-2"
          onClick={(e) => e.stopPropagation()}
        >
          {session.summary ? (
            <div className="text-sm text-foreground">
              <MarkdownEditor
                markdown={session.summary}
                editable={false}
                onChange={() => {}}
              />
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              This run left no summary.
            </p>
          )}
          {canResume && (
            <div>
              <Button
                variant="outline"
                size="sm"
                disabled={resuming}
                onClick={resume}
              >
                {resuming ? (
                  <LoaderCircle className="animate-spin" />
                ) : (
                  <ResumeIcon />
                )}
                Resume
              </Button>
            </div>
          )}
        </div>
      )}
    </GlassRow>
  )
}
