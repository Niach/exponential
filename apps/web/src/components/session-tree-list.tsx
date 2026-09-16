import { useMemo, useState } from "react"
import type { Board, CodingSession, Issue } from "@/db/schema"
import type { SessionDevice } from "@/lib/session-device"
import { nestSessions, visibleTreeRows } from "@/lib/session-tree"
import { sessionIdentity } from "@/lib/session-identity"
import { pastRunRowByline } from "@/components/agent-session-row"
import {
  PastSessionRow,
  RunningSessionRow,
} from "@/components/session-list-rows"
import type {
  SessionListRow,
  SessionMergeTarget,
} from "@/hooks/use-agents-data"

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
// exactly rule 2 of `session-tree.ts`.

export function SessionTreeList({
  rows,
  activeSessionId = null,
  onOpen,
  emptyNote,
  titleOf,
}: {
  /** The rows to nest, in the caller's ROOT order. */
  rows: readonly TreeListRow[]
  activeSessionId?: string | null
  onOpen: (session: CodingSession) => void
  /** Shown instead of the rows when there are none. Absent = render nothing. */
  emptyNote?: string
  /** A row's title when the caller knows better than the identity (the
   *  Automations tab resolves a deleted action's live name). */
  titleOf?: (row: TreeListRow) => string
}) {
  // Expanded by default, per parent, for as long as the list is mounted.
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  const byId = useMemo(
    () => new Map(rows.map((row) => [row.session.id, row])),
    [rows]
  )
  const nested = useMemo(
    () => nestSessions(rows.map((row) => row.session)),
    [rows]
  )
  const tree = useMemo(
    () => visibleTreeRows(nested, collapsed),
    [nested, collapsed]
  )
  const toggle = (sessionId: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(sessionId)) next.add(sessionId)
      return next
    })

  if (tree.length === 0) {
    return emptyNote ? (
      <div className="px-3 py-2 text-xs text-muted-foreground">{emptyNote}</div>
    ) : null
  }

  return (
    <div className="flex flex-col">
      {tree.map(({ session, depth, hasChildren }) => {
        const row = byId.get(session.id)
        if (!row) return null
        const active = session.id === activeSessionId
        const expanded = !collapsed.has(session.id)
        return session.status === `ended` ? (
          <PastSessionRow
            key={session.id}
            sessionId={session.id}
            title={titleOf?.(row) ?? row.title ?? sessionIdentity(row).subject}
            identifier={row.identifier ?? sessionIdentity(row).identifier}
            byline={pastRunRowByline(row)}
            depth={depth}
            active={active}
            expandable={hasChildren}
            expanded={expanded}
            onToggle={() => toggle(session.id)}
            onOpen={() => onOpen(session)}
          />
        ) : (
          <RunningSessionRow
            key={session.id}
            row={{ ...row, paused: row.paused ?? false } as SessionListRow}
            depth={depth}
            active={active}
            expandable={hasChildren}
            expanded={expanded}
            onToggle={() => toggle(session.id)}
            onOpen={() => onOpen(session)}
          />
        )
      })}
    </div>
  )
}
