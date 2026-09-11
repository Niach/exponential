import { Link, useParams } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { conceptIcon } from "@/lib/icons.generated"
import {
  mergeTargetProps,
  rowPrState,
  type AgentSessionRow,
  type PastRunRow,
} from "@/hooks/use-agents-data"
import type { CodingSession, SyncedAction } from "@/db/schema"
import {
  sessionDisplayState,
  type SessionDisplayState,
} from "@/components/issue-coding-rows"
import { relativeTime } from "@/components/comment-rows/format"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import { useNow } from "@/hooks/use-now"
import { pastRunByline, pastRunEndedAt } from "@/lib/past-runs"
import { actionCollection } from "@/lib/collections"
import { getActionIcon } from "@/lib/board-icons"
import { Button } from "@/components/ui/button"
import { Pill } from "@/components/ui/pill"
import { SessionMergeButton } from "@/components/session-merge-button"
import { ListRow } from "@/components/ui/glass-rows"

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
  steerEnabled = false,
  onOpen,
}: {
  row: AgentSessionRow
  teamSlug: string
  /** EXP-694: the caller's team role, from `useTeamPermissions` — it decides
   * WHICH editor the trailing button opens (a row must not resolve it itself:
   * the hook fetches the team's billing plan, once per row). */
  isOwner: boolean
  /** EXP-706: resolved ONCE by the caller (same reason as `isOwner`) — it
   * lets a conflicted Merge swap itself for the "Fix conflicts" run. */
  steerEnabled?: boolean
  onOpen: () => void
}) {
  const { session, issue, board } = row
  const isAction = session.actionName != null
  const isBatch = !session.issueId
  // EXP-535/EXP-734: batch rows merge through their resolved PR's
  // representative issue, an issue-less run through its own chore PR row
  // (use-agents-data) — same button either way.
  const mergeTarget = row.mergeTarget
  const displayState = sessionDisplayState(session, rowPrState(session, issue))
  // EXP-804: null unless the device reported the agent's usage wall on this
  // row. Orthogonal to `displayState`, so it renders alongside it.
  const blockedLabel = blockedBadgeLabel(session.blocked, useNow(30_000))
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
    <ListRow
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
            {/* EXP-746: the same words the Past rows and the three native
                clients use for an issue-less run (`lib/past-runs.ts`). */}
            {isAction
              ? session.actionName
              : isBatch
                ? `Batch run`
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
          {/* EXP-804: the agent's usage wall, BESIDE the state and never
              instead of it — a walled run is still running, just unable to
              make a call. Amber, the same attention tone "Needs input" uses,
              and absent entirely when the run is not blocked. */}
          {blockedLabel && (
            <span className="shrink-0 font-medium text-amber-400">
              {blockedLabel}
            </span>
          )}
          <span className="truncate">
            {`${device.label || session.deviceLabel || `Desktop`}${paused ? ` (offline)` : ``} · started ${relativeTime(session.startedAt)}`}
          </span>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {mergeTarget && (
          <SessionMergeButton
            {...mergeTargetProps(mergeTarget)}
            steerEnabled={steerEnabled}
          />
        )}
        {issue && board ? (
          <Pill asChild size="sm" mode="action" className="font-mono">
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
          </Pill>
        ) : editsAutomation && session.automationId ? (
          <Button asChild variant="glass" size="icon-sm">
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
          <Button asChild variant="glass" size="icon-sm">
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
    </ListRow>
  )
}

// ── Ended runs (EXP-637) ─────────────────────────────────────────────────────
// EXP-773: a finished run is a LINK, nothing more. The row used to expand into
// the agent's close-out summary and a Resume button; both moved into the
// fullscreen session view, where the transcript lives (the device republishes
// its journal on demand). So Devices → Past, the chat page's "Past chats" and
// the Automations tab's "Recent automated runs" all render the same plain row:
// title, identifier, byline, and a tap that opens `sessions/$sessionId`.
// Mirrored on desktop, iOS and Android.

/** EXP-746: the Past row's caption. The ORDER and the separator are the ×4
 * rule (lib/past-runs.ts); the relative time is this client's own formatter.
 * Lives here (EXP-739) so the Devices "Past" list and the chat page's "Past
 * chats" caption identically. */
export function pastRunRowByline(row: PastRunRow): string {
  // A row that stamped neither end nor heartbeat has no honest time to show
  // (0 would render as 1970), so that segment simply drops.
  const endedAt = pastRunEndedAt(row.session)
  return pastRunByline({
    deviceLabel: row.device.label ?? row.session.deviceLabel,
    relativeTime: endedAt > 0 ? relativeTime(new Date(endedAt)) : ``,
  })
}

/** What an ended-run row needs — the Automations tab builds it off the synced
 * session row. */
export interface EndedRunRow {
  session: CodingSession
}

export function EndedSessionRow({
  row,
  /** The title line — the action that fired (the row's `actionName`
   * snapshot, or the live action's name when the snapshot is missing). */
  title,
  identifier,
  byline,
}: {
  row: EndedRunRow
  title: string
  /** EXP-746: an issue run's identifier, drawn as the mono lead-in
   * `SessionRow` already uses (iOS/Android `EndedRunRow` parity). */
  identifier?: string
  /** EXP-746: the Past caption — `<device> · <rel time>` (lib/past-runs.ts
   * `pastRunByline`, trimmed by EXP-833). It ALREADY ends with the
   * relative time, so the trailing time column stands down when it is set;
   * the Automations tab passes none and keeps the old row verbatim. */
  byline?: string
}) {
  const { session } = row
  // Loose match, like `useOpenSession`: every caller lives under `/t/$teamSlug`,
  // but the row must not throw in a test that mounts one outside the layout.
  const { teamSlug } = useParams({ strict: false })

  const body = (
    <>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          {identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {identifier}
            </span>
          )}
          <span className="truncate font-medium">{title}</span>
        </div>
        {byline && (
          <div className="truncate text-xs text-muted-foreground">{byline}</div>
        )}
      </div>
      {!byline && (
        <span className="shrink-0 text-xs text-muted-foreground">
          {relativeTime(session.endedAt ?? session.startedAt)}
        </span>
      )}
    </>
  )

  if (!teamSlug) {
    return (
      <ListRow
        className="gap-2"
        data-testid={`ended-session-${session.id}`}
      >
        {body}
      </ListRow>
    )
  }

  return (
    <ListRow asChild interactive className="gap-2">
      <Link
        to="/t/$teamSlug/sessions/$sessionId"
        params={{ teamSlug, sessionId: session.id }}
        data-testid={`ended-session-${session.id}`}
      >
        {body}
      </Link>
    </ListRow>
  )
}
