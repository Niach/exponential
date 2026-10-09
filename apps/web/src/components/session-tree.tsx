import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import {
  getDeviceIcon,
  SessionRow,
  treeGuides,
  type SessionRowSize,
  type TreeGuide,
} from "@exp/ui"
import type { Board, CodingSession, Device, Issue } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import type { SessionDevice } from "@/lib/session-device"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import {
  runningRowMarkState,
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"
import { runHasEnded } from "@/lib/past-runs"
import { sessionIdentity } from "@/lib/session-identity"
import { sessionRowCaption } from "@/lib/session-row-caption"
import {
  sessionTree,
  visibleSessionTreeRows,
  type SessionTreeFlatRow,
} from "@/lib/sessions/session-tree"
import { rowPrState, type SessionMergeTarget } from "@/hooks/use-agents-data"
import { useNow } from "@/hooks/use-now"

// EXP-996: the session list as a TREE, not a flat roll of strangers. The list
// draws the `sessionTree` selector (`lib/sessions/session-tree.ts`, the x4
// rule): a resume succession is ONE row and a run started by another run
// nests under it.
//
// EXP-1248: every row is `@exp/ui` SessionRow (small in the sidebar, big
// everywhere else) and children ALWAYS show: no fold, so a parent's mark and
// title line up exactly with its child's and a standalone row's.
//
// The CAP belongs to the caller, applied to the ROWS before they get here
// (`PAST_RUN_CAP`, an action's runs): a tree built from a truncated list
// leaves an unlisted parent's child a root (rule 4 of the selector).

/** What this list needs of a row. `SessionListRow` (the joined running rows)
 *  and `PastRunRow` (Recent's rows, which carry their own title) both satisfy
 *  it, so one component serves every band. */
export interface TreeListRow {
  session: CodingSession
  issue: Issue | undefined
  /** EXP-876: a batch row's covered issues — what names it. */
  batchIssues?: Issue[]
  board: Board | undefined
  device: SessionDevice
  paused?: boolean
  mergeTarget?: SessionMergeTarget
  /** Recent's precomputed title; absent = derive it from the identity. */
  title?: string
  /** Recent's precomputed lead-in; `undefined` = derive it from the identity
   *  (which is where a batch's `EXP-874 +2` comes from). */
  identifier?: string | null
}

const NO_FOLDS: ReadonlySet<string> = new Set<string>()

/** EXP-996: a session tree's rows in draw order, with the EXP-965 connector
 *  geometry: the one model behind every drawn list. */
export function useSessionTreeRows<T extends TreeListRow>(
  rows: readonly T[],
  // Kept for old callers; EXP-1248 lists never fold.
  _collapsed: ReadonlySet<string> = NO_FOLDS
): { flat: SessionTreeFlatRow<CodingSession>; row: T | undefined; guide: TreeGuide }[] {
  return useMemo(() => {
    const byId = new Map(rows.map((row) => [row.session.id, row]))
    const visible = visibleSessionTreeRows(
      sessionTree(rows.map((row) => row.session)),
      NO_FOLDS
    )
    const guides = treeGuides(visible.map((entry) => entry.depth))
    return visible.map((flat, index) => ({
      flat,
      row: byId.get(flat.node.session.id),
      guide: guides[index]!,
    }))
  }, [rows])
}

type DeviceGlyphRow = Pick<Device, `deviceId` | `userId` | `icon` | `kind`>

/** The synced devices row a session runs on (the owner's own row first),
 *  for its icon. */
export function sessionDeviceRow<D extends DeviceGlyphRow>(
  session: Pick<CodingSession, `deviceId` | `userId`>,
  devices: readonly D[]
): D | undefined {
  if (!session.deviceId) return undefined
  const matches = devices.filter((d) => d.deviceId === session.deviceId)
  return matches.find((d) => d.userId === session.userId) ?? matches[0]
}

/** Every synced device row, for the rows' trailing device icon. */
export function useDeviceRows(): Device[] {
  const { data } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  return (data ?? []) as Device[]
}

/** What one drawn row says: the SessionRow props a TreeListRow resolves to. */
export function sessionRowFacts(
  row: TreeListRow,
  { now, title }: { now: Date; title?: string }
) {
  const { session } = row
  const identity = sessionIdentity(row)
  const ended = runHasEnded(session)
  const paused = !ended && Boolean(row.paused)
  const prState = rowPrState(session, row.issue)
  const state = sessionDisplayState(session, prState)
  const deviceName = row.device.label || session.deviceLabel || null
  const caption = sessionRowCaption({
    ended,
    paused,
    state,
    device: deviceName,
    startedAt: session.startedAt,
    updatedAt: session.updatedAt,
    endedAt: session.endedAt,
    // EXP-804: the agent's usage wall wins the caption while it lasts.
    blockedLabel: ended ? null : blockedBadgeLabel(session.blocked, now),
    now,
  })
  return {
    agent: session.agent,
    markState: ended
      ? (`ended` as const)
      : runningRowMarkState(state, {
          paused,
          working: sessionRowIsWorking(session, prState),
        }),
    identifier: row.identifier ?? identity.identifier,
    title: title ?? row.title ?? identity.subject,
    caption: caption.text,
    captionTone: caption.tone,
    deviceName,
    paused,
  }
}

// EXP-897: the ONE nested session list: the phone Agent page's Running, the
// Recent panel and sheet, an action's Runs and its second-sidebar list.
export function SessionTree({
  rows,
  activeSessionId = null,
  onOpen,
  emptyNote,
  titleOf,
  ringClassName,
  size = `big`,
}: {
  /** The rows to nest, in the caller's order. */
  rows: readonly TreeListRow[]
  activeSessionId?: string | null
  onOpen: (session: CodingSession) => void
  /** Shown instead of the rows when there are none. Absent = render nothing. */
  emptyNote?: string
  /** A row's title when the caller knows better than the identity. */
  titleOf?: (row: TreeListRow) => string
  /** EXP-1208: the run mark badge's ring = the ground the list sits on
   *  (`ring-sidebar` in a sidebar panel); absent = the page background. */
  ringClassName?: string
  size?: SessionRowSize
}) {
  const tree = useSessionTreeRows(rows)
  const devices = useDeviceRows()
  const now = useNow(30_000)

  if (tree.length === 0) {
    return emptyNote ? (
      <div className="px-3 py-2 text-xs text-muted-foreground">{emptyNote}</div>
    ) : null
  }

  return (
    // Gapless (EXP-965: nothing for a connector to bridge).
    <div className="flex flex-col">
      {tree.map(({ flat, row, guide }) => {
        if (!row) return null
        const { session } = row
        const facts = sessionRowFacts(row, { now, title: titleOf?.(row) })
        return (
          <SessionRow
            key={flat.key}
            size={size}
            agent={facts.agent}
            markState={facts.markState}
            identifier={facts.identifier}
            title={facts.title}
            caption={facts.caption}
            captionTone={facts.captionTone}
            depth={flat.depth}
            guide={guide}
            deviceIcon={getDeviceIcon(sessionDeviceRow(session, devices) ?? {})}
            deviceName={facts.deviceName}
            active={session.id === activeSessionId}
            dimmed={facts.paused}
            ringClassName={ringClassName}
            onClick={() => onOpen(session)}
            data-testid={`session-row-${row.issue?.identifier ?? session.id}`}
          />
        )
      })}
    </div>
  )
}
