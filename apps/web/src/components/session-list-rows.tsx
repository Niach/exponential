import {
  AgentRunMark,
  conceptIcon,
  ListRow,
  type RunMarkState,
  TREE_BASE,
  TREE_INDENT,
  TreeGuides,
  type TreeGuide,
} from "@exp/ui"
import {
  sessionAgentCaption,
  sessionDisplayState,
  sessionRowIsWorking,
  sessionStatusLine,
  type SessionDisplayState,
  type SessionStatusTone,
} from "@/lib/coding-session-display"
import { sessionIdentity } from "@/lib/session-identity"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import { cn } from "@/lib/utils"
import { rowPrState, type SessionListRow } from "@/hooks/use-agents-data"
import { useNow } from "@/hooks/use-now"

// EXP-874: the ONE session list row layout — the sidebar's Agent list nav, the
// Agent page and both Automations run lists render these two rows. Android's
// session row is the reference: a flat row, the state dot, identifier + title,
// the agent caption, the status line. EXP-893: no trailing buttons — the
// Merge / Fix-conflicts circle and the open-issue / open-action circle are
// gone on every client; a row only opens the run (Merge lives on the run's
// Changes face, the issue behind its Issue face).
// EXP-876: a batch run fills those same two slots instead of reading "Batch
// run" — `EXP-874 +2` beside its first covered issue's title. Which also
// answers the trailing control that question came with: still none, since an
// open-issue circle was never a thing a multi-issue run could point at.
//
// EXP-1208: the row order is [run mark][fold chevron, parents only][text], ×4.
// The mark (`@exp/ui` `AgentRunMark`, never a dot) sits at the row's base
// inset `12 + 14·depth` on EVERY row, so a parent's mark lines up exactly
// with a standalone row's and a child's connector elbow ends at its mark (the
// EXP-965 gutter centre is the mark's centre). The chevron FOLLOWS the mark;
// the sub-lines align under the title, never under the mark.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

/** EXP-1208: a live row's mark state — the ×4 display state, except that a
 *  paused run (offline host) wears the bare mark and only a row that is
 *  WORKING animates (EXP-848: `sessionRowIsWorking`, never `running` alone). */
export function runningRowMarkState(
  state: SessionDisplayState,
  { paused, working }: { paused: boolean; working: boolean }
): RunMarkState | undefined {
  if (paused) return undefined
  if (state === `working`) return working ? `working` : undefined
  return state
}

/** The fold chevron of a parent row: 14px wide, AFTER the run mark. */
function RowFoldToggle({
  expanded,
  onToggle,
}: {
  expanded: boolean
  onToggle?: () => void
}) {
  return (
    <span
      role="button"
      tabIndex={-1}
      aria-label={expanded ? `Collapse child runs` : `Expand child runs`}
      data-slot="session-row-fold"
      className="flex w-3.5 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
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
  )
}

/** What a row adds to the two rows below — a title that beats the identity
 *  (a caller's own, an action's Runs). A plain run passes none. */
export interface SessionRowDecor {
  title?: string
}

/** The default ground a session list sits on (the Agent page, an action's
 *  Runs); the sidebar's list nav passes `ring-sidebar`. */
const RUN_MARK_RING = `ring-background`

const TONE_CLASS: Record<SessionStatusTone, string> = {
  muted: `text-muted-foreground`,
  amber: `text-amber-400`,
  emerald: `text-emerald-400`,
  sky: `text-sky-400`,
}

export function RunningSessionRow({
  row,
  depth = 0,
  guide = null,
  active = false,
  expandable = false,
  expanded = true,
  onToggle,
  onOpen,
  decor,
  ringClassName = RUN_MARK_RING,
}: {
  row: SessionListRow
  /** EXP-1068: the row's additions; absent on a plain run. */
  decor?: SessionRowDecor
  /** Nesting depth under a parent run — 14px of indent per level. */
  depth?: number
  /** EXP-965: this row's connector geometry (`treeGuides(depths)[index]`). */
  guide?: TreeGuide | null
  active?: boolean
  /** EXP-849: this run started others — the row carries a fold chevron. */
  expandable?: boolean
  expanded?: boolean
  onToggle?: () => void
  onOpen: () => void
  /** The run mark badge's ring = the ground the row sits on. */
  ringClassName?: string
}) {
  const { session, issue, device, paused } = row
  // EXP-876: a batch row's identity comes off the issues it covers
  // (`EXP-874 +2`), which is the only thing telling two batches apart.
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
      className={cn(`relative gap-2 px-3 py-2.5`, paused && `opacity-60`)}
      style={{ paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px` }}
      title={paused ? `${device.label ?? `The device`} is offline` : undefined}
      data-testid={`session-row-${issue?.identifier ?? session.id}`}
    >
      <TreeGuides guide={guide} />
      <AgentRunMark
        agent={session.agent}
        state={runningRowMarkState(state, { paused, working })}
        ringClassName={ringClassName}
      />
      {expandable && <RowFoldToggle expanded={expanded} onToggle={onToggle} />}
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          {identity.identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {identity.identifier}
            </span>
          )}
          <span className="truncate font-medium">{decor?.title ?? identity.subject}</span>
        </div>
        {caption && (
          <div
            className="truncate text-xs text-muted-foreground"
            title={caption}
          >
            {caption}
          </div>
        )}
        <div className={cn(`truncate text-xs`, TONE_CLASS[status.tone])}>
          {status.text}
        </div>
        {blockedLabel && (
          <div className="truncate text-xs font-medium text-amber-400">
            {blockedLabel}
          </div>
        )}
      </div>
    </ListRow>
  )
}

/** A finished run: its dimmed run mark, identity + byline, a chevron, and the
 *  whole row opens it.
 *  EXP-897: it nests and folds exactly like the running row — an ended
 *  orchestrator's children are its children on every list ×4. */
export function PastSessionRow({
  sessionId,
  agent,
  title,
  identifier,
  byline,
  depth = 0,
  guide = null,
  active = false,
  expandable = false,
  expanded = true,
  onToggle,
  onOpen,
  ringClassName = RUN_MARK_RING,
}: {
  sessionId: string
  /** The run's `coding_sessions.agent` — whose brand mark leads the row. */
  agent: string | null | undefined
  title: string
  /** An issue run's identifier, or a batch's `EXP-874 +2` (EXP-876) — the
   *  mono lead-in; null for an action or chat run. */
  identifier: string | null
  /** `pastRunRowByline(row)` — `<device> · <rel time>`. */
  byline: string
  /** Nesting depth under a parent run — 14px of indent per level. */
  depth?: number
  /** EXP-965: this row's connector geometry (`treeGuides(depths)[index]`). */
  guide?: TreeGuide | null
  active?: boolean
  /** EXP-849: this run started others — the row carries a fold chevron. */
  expandable?: boolean
  expanded?: boolean
  onToggle?: () => void
  onOpen: () => void
  /** The run mark badge's ring = the ground the row sits on. */
  ringClassName?: string
}) {
  return (
    <ListRow
      interactive
      active={active}
      onClick={onOpen}
      className="relative gap-2 px-3 py-2.5"
      style={{ paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px` }}
      data-testid={`session-row-${sessionId}`}
    >
      <TreeGuides guide={guide} />
      <AgentRunMark
        agent={agent}
        state="ended"
        ringClassName={ringClassName}
      />
      {expandable && <RowFoldToggle expanded={expanded} onToggle={onToggle} />}
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
      {/* The trailing chevron is decoration: the whole row opens the run, and
          the LEADING one is the fold control that owns the a11y name. */}
      <ChevronRightIcon
        aria-hidden
        className="size-4 shrink-0 text-muted-foreground"
      />
    </ListRow>
  )
}
