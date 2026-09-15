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
import { cn } from "@/lib/utils"
import { rowPrState, type SessionListRow } from "@/hooks/use-agents-data"
import { useNow } from "@/hooks/use-now"
import { RunningIndicator } from "@/components/agent-session-row"
import { ListRow } from "@/components/ui/glass-rows"

// EXP-874: the ONE session list row layout — the sidebar's Agent list nav, the
// Agent page and both Automations run lists render these two rows. Android's
// session row is the reference: a flat row, the state dot, identifier + title,
// the agent caption, the status line. EXP-893: no trailing buttons — the
// Merge / Fix-conflicts circle and the open-issue / open-action circle are
// gone on every client; a row only opens the run (Merge lives on the run's
// Changes face, the issue behind its Issue face).

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

const TONE_CLASS: Record<SessionStatusTone, string> = {
  muted: `text-muted-foreground`,
  amber: `text-amber-400`,
  emerald: `text-emerald-400`,
  sky: `text-sky-400`,
}

export function RunningSessionRow({
  row,
  depth = 0,
  active = false,
  expandable = false,
  expanded = true,
  onToggle,
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
  onOpen: () => void
}) {
  const { session, issue, device, paused } = row
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
