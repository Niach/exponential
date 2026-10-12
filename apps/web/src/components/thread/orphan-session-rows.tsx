import { getDeviceIcon, SessionRow } from "@exp/ui"
import type { CodingSession } from "@/db/schema"
import {
  sessionDeviceRow,
  sessionRowFacts,
  useDeviceRows,
} from "@/components/session-tree"
import { useSessionListRows } from "@/hooks/use-agents-data"
import { useNow } from "@/hooks/use-now"
import { useOpenSession } from "@/hooks/use-open-session"
import type { DetailOrigin } from "@/lib/detail-origin"

// EXP-1284 (review T4): a live session of mine that no listed thread holds —
// its thread item failed to place, or the items shape lags the sessions
// shape — still shows in Running, as a PLAIN session row (the big
// SessionRow: mark, identifier + title, caption, host device) that opens the
// session's own page. A live session never vanishes from the lists.

export function OrphanSessionRows({
  teamId,
  sessions,
  origin,
  ringClassName,
  activeSessionId = null,
}: {
  teamId: string | undefined
  sessions: readonly CodingSession[]
  origin: DetailOrigin | null
  ringClassName?: string
  activeSessionId?: string | null
}) {
  const rows = useSessionListRows(teamId, sessions)
  const devices = useDeviceRows()
  const now = useNow(30_000)
  const openSession = useOpenSession()
  if (rows.length === 0) return null
  return (
    <>
      {rows.map((row) => {
        const facts = sessionRowFacts(row, { now })
        return (
          <SessionRow
            key={row.session.id}
            size="big"
            agent={facts.agent}
            markState={facts.markState}
            identifier={facts.identifier}
            title={facts.title}
            caption={facts.caption}
            captionTone={facts.captionTone}
            deviceIcon={getDeviceIcon(sessionDeviceRow(row.session, devices) ?? {})}
            deviceName={facts.deviceName}
            dimmed={facts.paused}
            active={row.session.id === activeSessionId}
            ringClassName={ringClassName}
            onClick={() => openSession(row.session, { origin })}
            data-testid={`orphan-session-${row.session.id}`}
          />
        )
      })}
    </>
  )
}
