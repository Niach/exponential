import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { GlassSectionHeader } from "@exp/ui"
import { SessionTree } from "@/components/session-tree"
import { useSessionListRows } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import { actionRunTitle } from "@/lib/action-triggers"
import { codingSessionCollection } from "@/lib/collections"
import type { CodingSession } from "@/db/schema"

// An action's run history (SLOP-2): every `coding_sessions` row that ran it,
// newest first, each titled by what started it (a schedule, an event or a
// person). It replaces the Automations page's "Recent
// automated runs": a triggered run is listed nowhere else.

export function ActionRunsSection({
  actionId,
  teamId,
  showHeader = true,
}: {
  actionId: string
  teamId: string
  /** Off on a phone, where the tab already says "Runs". */
  showHeader?: boolean
}) {
  const openSession = useOpenSession()
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ sessions: codingSessionCollection })
        .where(({ sessions }) => eq(sessions.actionId, actionId)),
    [actionId]
  )
  const runs = useMemo(
    () =>
      [...((sessionRows ?? []) as CodingSession[])].sort(
        (a, b) =>
          new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
      ),
    [sessionRows]
  )
  const rows = useSessionListRows(teamId, runs)

  return (
    <div>
      {showHeader && <GlassSectionHeader label="Runs" />}
      {runs.length === 0 ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">
          No runs yet.
        </div>
      ) : (
        /* EXP-897: nested by `parent_session_id`, like every session list. */
        <SessionTree
          rows={rows}
          titleOf={(row) => actionRunTitle(row.session.startedReason)}
          // Opened from this action, so Back returns to its Runs.
          onOpen={(session) =>
            openSession(session, { origin: { kind: `action`, actionId } })
          }
        />
      )}
    </div>
  )
}
