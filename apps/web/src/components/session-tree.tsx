import { useMemo, useState } from "react"
import { conceptIcon, treeGuides, type TreeGuide } from "@exp/ui"
import type { Board, CodingSession, Issue } from "@/db/schema"
import type { SessionDevice } from "@/lib/session-device"
import { runHasEnded } from "@/lib/past-runs"
import { sessionIdentity } from "@/lib/session-identity"
import {
  sessionTree,
  visibleSessionTreeRows,
  type SessionTreeFlatRow,
} from "@/lib/sessions/session-tree"
import { pastRunRowByline } from "@/components/agent-session-row"
import {
  PastSessionRow,
  RunningSessionRow,
} from "@/components/session-list-rows"
import type {
  SessionListRow,
  SessionMergeTarget,
} from "@/hooks/use-agents-data"

// EXP-996: the session list as a TREE, not a flat roll of strangers. The list
// draws the `sessionTree` selector (`lib/sessions/session-tree.ts`, the ×4
// rule): a resume succession is ONE row and a run started by another run
// nests under it. A run parked on an open question wears the red dot.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

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

/** EXP-996: the VISIBLE rows of a session tree, with the EXP-965 connector
 *  geometry — the one model behind every drawn list. The connector reads off
 *  the visible depths, so a folded subtree simply is not there. */
export function useSessionTreeRows<T extends TreeListRow>(
  rows: readonly T[],
  collapsed: ReadonlySet<string>
): { flat: SessionTreeFlatRow<CodingSession>; row: T | undefined; guide: TreeGuide }[] {
  return useMemo(() => {
    const byId = new Map(rows.map((row) => [row.session.id, row]))
    const visible = visibleSessionTreeRows(
      sessionTree(rows.map((row) => row.session)),
      collapsed
    )
    const guides = treeGuides(visible.map((entry) => entry.depth))
    return visible.map((flat, index) => ({
      flat,
      row: byId.get(flat.node.session.id),
      guide: guides[index]!,
    }))
  }, [rows, collapsed])
}

/** EXP-996: the collapse set every session list holds — expanded by default,
 *  per node, for as long as the list is mounted, keyed by the node key. */
export function useCollapsedNodes(): [
  ReadonlySet<string>,
  (key: string) => void,
] {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  const toggle = (key: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(key)) next.add(key)
      return next
    })
  return [collapsed, toggle]
}

/** The fold chevron of a parent run's row. */
export function TreeFoldToggle({
  expanded,
  label,
  onToggle,
}: {
  expanded: boolean
  label: string
  onToggle: () => void
}) {
  return (
    <span
      role="button"
      tabIndex={0}
      aria-label={label}
      aria-expanded={expanded}
      className="flex shrink-0 items-center justify-center text-muted-foreground hover:text-foreground focus-visible:text-foreground focus-visible:outline-none"
      onClick={(event) => {
        event.stopPropagation()
        onToggle()
      }}
      // A `role="button"` span gets none of a button's keys for free: Enter
      // and Space fold, and stay off the row underneath (its link, its open).
      onKeyDown={(event) => {
        if (event.key !== `Enter` && event.key !== ` `) return
        event.preventDefault()
        event.stopPropagation()
        onToggle()
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

// EXP-897: the ONE nested session list. EXP-818 gave the Agent page's Running
// band the tree; every OTHER list (the Agent page's Recent band, the sidebar's
// automated-runs nav, the Automations tab's "Recent automated runs") still
// listed a child run as a stranger. They all render this component now, so a
// run started by another run reads the same wherever it is listed — the ×4
// rule (desktop `sessions_section`, iOS `AgentSessionsList`, Android
// `AgentSessionsList.kt`).
//
// The CAP belongs to the caller, applied to the ROWS before they get here
// (`PAST_RUN_CAP`, the Automations tab's `slice(0, 10)`): a tree built from a
// truncated list simply leaves an unlisted parent's child a root, which is
// exactly rule 4 of `lib/sessions/session-tree.ts`.

export function SessionTree({
  rows,
  activeSessionId = null,
  onOpen,
  emptyNote,
  titleOf,
  ringClassName,
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
}) {
  const [collapsed, toggle] = useCollapsedNodes()
  const tree = useSessionTreeRows(rows, collapsed)

  if (tree.length === 0) {
    return emptyNote ? (
      <div className="px-3 py-2 text-xs text-muted-foreground">{emptyNote}</div>
    ) : null
  }

  return (
    // Gapless (EXP-965: nothing for a connector to bridge).
    <div className="flex flex-col">
      {tree.map(({ flat, row, guide }) => {
        const { node, key, depth, hasChildren } = flat
        const expanded = !collapsed.has(key)
        if (!row) return null
        const session = node.session
        const active = session.id === activeSessionId
        return runHasEnded(session) ? (
          <PastSessionRow
            key={key}
            sessionId={session.id}
            agent={session.agent}
            title={titleOf?.(row) ?? row.title ?? sessionIdentity(row).subject}
            identifier={row.identifier ?? sessionIdentity(row).identifier}
            byline={pastRunRowByline(row)}
            depth={depth}
            guide={guide}
            active={active}
            expandable={hasChildren}
            expanded={expanded}
            onToggle={() => toggle(key)}
            onOpen={() => onOpen(session)}
            ringClassName={ringClassName}
          />
        ) : (
          <RunningSessionRow
            key={key}
            row={{ ...row, paused: row.paused ?? false } as SessionListRow}
            // A caller's title names a LIVE row too (an action's Runs).
            decor={titleOf ? { title: titleOf(row) } : undefined}
            depth={depth}
            guide={guide}
            active={active}
            expandable={hasChildren}
            expanded={expanded}
            onToggle={() => toggle(key)}
            onOpen={() => onOpen(session)}
            ringClassName={ringClassName}
          />
        )
      })}
    </div>
  )
}
