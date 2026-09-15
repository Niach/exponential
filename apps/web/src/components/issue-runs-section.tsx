import { useState } from "react"
import { pastRunRowByline } from "@/components/agent-session-row"
import { PastSessionRow } from "@/components/session-list-rows"
import { GlassSectionHeader } from "@/components/ui/glass-rows"
import { useIssueRuns } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"

/**
 * EXP-886: the issue detail's "Runs" band — the caller's OWN finished runs of
 * this issue (`selectIssueRuns`: any started reason, newest first, uncapped),
 * so a run older than the Agent page's 20 Recent rows stays one click away.
 * The same fold as Recent, collapsed by default, no count; absent with no
 * rows. Rows are the Recent row and open the run exactly like it. ×4 lockstep.
 */
export function IssueRunsSection({
  issueId,
  teamId,
  currentUserId,
}: {
  issueId: string
  teamId: string
  currentUserId: string
}) {
  const { runs } = useIssueRuns(issueId, teamId, currentUserId)
  const openSession = useOpenSession()
  const [open, setOpen] = useState(false)
  if (runs.length === 0) return null
  return (
    <div className="px-4 pt-3 pb-2">
      <GlassSectionHeader
        label="Runs"
        expanded={open}
        onToggle={() => setOpen((value) => !value)}
      />
      {open && (
        <div className="flex flex-col">
          {runs.map((row) => (
            <PastSessionRow
              key={row.session.id}
              sessionId={row.session.id}
              title={row.title}
              identifier={row.identifier}
              byline={pastRunRowByline(row)}
              onOpen={() => openSession(row.session)}
            />
          ))}
        </div>
      )}
    </div>
  )
}
