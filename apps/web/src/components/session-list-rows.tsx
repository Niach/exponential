import { Link, useParams } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { SyncedAction } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import {
  sessionAgentCaption,
  sessionDisplayState,
  sessionRowIsWorking,
  sessionStatusLine,
  type SessionStatusTone,
} from "@/lib/coding-session-display"
import { sessionIdentity } from "@/lib/session-identity"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import { actionCollection } from "@/lib/collections"
import { getActionIcon } from "@/lib/board-icons"
import { cn } from "@/lib/utils"
import {
  mergeTargetProps,
  rowPrState,
  type SessionListRow,
} from "@/hooks/use-agents-data"
import { useNow } from "@/hooks/use-now"
import { RunningIndicator } from "@/components/agent-session-row"
import { SessionMergeButton } from "@/components/session-merge-button"
import { Button } from "@/components/ui/button"
import { ListRow } from "@/components/ui/glass-rows"

// EXP-874: the ONE session list row layout — the sidebar's Agent list nav, the
// Agent page and both Automations run lists render these two rows. Android's
// session row is the reference: a flat row, the state dot, identifier + title,
// the agent caption, the status line, and circular trailing buttons.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const IssueIcon = conceptIcon(`ui-issue`)
// EXP-530: the fallback glyph for an automation run whose action row has not
// synced (or was deleted).
const ActionAutomationIcon = conceptIcon(`action-automation`)

const TONE_CLASS: Record<SessionStatusTone, string> = {
  muted: `text-muted-foreground`,
  amber: `text-amber-400`,
  emerald: `text-emerald-400`,
  sky: `text-sky-400`,
}

// The trailing buttons are their own targets: the row's click (open the run)
// must not fire underneath them.
const stop = (event: { stopPropagation: () => void }) =>
  event.stopPropagation()

/** The action a run belongs to, off the synced (body-less) `actions` shape —
 * it carries the glyph the trailing button draws. Issue/chat/batch rows have
 * no `actionId` and run no query at all. */
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

export function RunningSessionRow({
  row,
  depth = 0,
  active = false,
  expandable = false,
  expanded = true,
  onToggle,
  isOwner = false,
  steerEnabled = false,
  onOpen,
}: {
  row: SessionListRow
  /** Nesting depth under a parent run — 14px of indent per level. */
  depth?: number
  active?: boolean
  /** EXP-849: this run started others — the row carries a fold chevron. */
  expandable?: boolean
  expanded?: boolean
  onToggle?: () => void
  /** EXP-694: the caller's team role, resolved ONCE by the list — it decides
   * whether an automation run's button edits the automation or the action. */
  isOwner?: boolean
  /** EXP-706: member + relay configured — lets a conflicted Merge swap
   * itself for the "Fix conflicts" run. */
  steerEnabled?: boolean
  onOpen: () => void
}) {
  const { session, issue, board, device, paused, mergeTarget } = row
  // Loose match: the row must not throw when mounted outside `/t/$teamSlug`.
  const { teamSlug } = useParams({ strict: false })
  const identity = sessionIdentity(row)
  const prState = rowPrState(session, issue)
  const state = sessionDisplayState(session, prState)
  // EXP-848: the ping keys on the device-written turn flag, never on `running`.
  const working = sessionRowIsWorking(session, prState)
  const caption = sessionAgentCaption(session)
  // EXP-804: the agent's usage wall — orthogonal to the state line.
  const blockedLabel = blockedBadgeLabel(session.blocked, useNow(30_000))
  const deviceName = device.label || session.deviceLabel || `Desktop`
  const status = sessionStatusLine({
    state,
    paused,
    device: deviceName,
    startedAt: session.startedAt,
  })

  // An automation run edits the AUTOMATION only for a team owner (that form
  // has no read-only mode); every other member lands in the action editor.
  const runAction = useRunAction(session.actionId)
  const editsAutomation = Boolean(session.automationId) && isOwner
  const RunIcon = runAction
    ? getActionIcon(runAction)
    : editsAutomation
      ? ActionAutomationIcon
      : getActionIcon({})

  return (
    <ListRow
      interactive
      active={active}
      onClick={onOpen}
      className={cn(`gap-2 px-3 py-2.5`, paused && `opacity-60`)}
      style={depth > 0 ? { paddingLeft: `${12 + depth * 14}px` } : undefined}
      title={paused ? `${device.label ?? `The device`} is offline` : undefined}
      data-testid={`session-row-${issue?.identifier ?? session.id}`}
    >
      {expandable && (
        <span
          role="button"
          tabIndex={-1}
          aria-label={expanded ? `Collapse child runs` : `Expand child runs`}
          className="flex shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
          onClick={(event) => {
            event.stopPropagation()
            onToggle?.()
          }}
        >
          {expanded ? (
            <ChevronDownIcon className="size-3" />
          ) : (
            <ChevronRightIcon className="size-3" />
          )}
        </span>
      )}
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="flex shrink-0 items-center justify-center">
            <RunningIndicator state={state} paused={paused} working={working} />
          </span>
          {issue && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {issue.identifier}
            </span>
          )}
          <span className="truncate font-medium">{identity.subject}</span>
        </div>
        {caption && (
          <div
            className="truncate pl-3.5 text-xs text-muted-foreground"
            title={caption}
          >
            {caption}
          </div>
        )}
        <div className={cn(`truncate pl-3.5 text-xs`, TONE_CLASS[status.tone])}>
          {status.text}
        </div>
        {blockedLabel && (
          <div className="truncate pl-3.5 text-xs font-medium text-amber-400">
            {blockedLabel}
          </div>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {mergeTarget && (
          <SessionMergeButton
            {...mergeTargetProps(mergeTarget)}
            variant="outline"
            size="icon-sm"
            className="rounded-full"
            steerEnabled={steerEnabled}
          />
        )}
        {teamSlug && issue && board ? (
          <Button
            asChild
            variant="outline"
            size="icon-sm"
            className="rounded-full"
          >
            <Link
              to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
              params={{
                teamSlug,
                boardSlug: board.slug,
                issueIdentifier: issue.identifier,
              }}
              onClick={stop}
              aria-label={`Open ${issue.identifier}`}
              title={issue.identifier}
            >
              <IssueIcon className="size-4" />
            </Link>
          </Button>
        ) : teamSlug && editsAutomation && session.automationId ? (
          <Button
            asChild
            variant="outline"
            size="icon-sm"
            className="rounded-full"
          >
            <Link
              to="/t/$teamSlug/actions"
              params={{ teamSlug }}
              search={{
                tab: `automations`,
                editAutomation: session.automationId,
              }}
              onClick={stop}
              aria-label="Edit automation"
              title="Edit automation"
            >
              <RunIcon className="size-4" />
            </Link>
          </Button>
        ) : teamSlug && session.actionId ? (
          <Button
            asChild
            variant="outline"
            size="icon-sm"
            className="rounded-full"
          >
            <Link
              to="/t/$teamSlug/actions"
              params={{ teamSlug }}
              search={{ editAction: session.actionId }}
              onClick={stop}
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

/** A finished run: identity + byline, a chevron, and the whole row opens it. */
export function PastSessionRow({
  sessionId,
  title,
  identifier,
  byline,
  active = false,
  onOpen,
}: {
  sessionId: string
  title: string
  /** An issue run's identifier — the mono lead-in; null for every other kind. */
  identifier: string | null
  /** `pastRunRowByline(row)` — `<device> · <rel time>`. */
  byline: string
  active?: boolean
  onOpen: () => void
}) {
  return (
    <ListRow
      interactive
      active={active}
      onClick={onOpen}
      className="gap-2 px-3 py-2.5"
      data-testid={`session-row-${sessionId}`}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          {identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {identifier}
            </span>
          )}
          <span className="truncate">{title}</span>
        </div>
        {byline && (
          <div className="truncate text-xs text-muted-foreground">{byline}</div>
        )}
      </div>
      <ChevronRightIcon className="size-4 shrink-0 text-muted-foreground" />
    </ListRow>
  )
}
