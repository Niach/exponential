import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Board, CodingSession, Issue } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { codingSessionCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { useCanResumeOn } from "@/hooks/use-resume-run"
import { IssueCodingControl } from "@/components/issue-coding-rows"
import {
  ResumeRunPill,
  SessionStopRunPill,
} from "@/components/run-action-pills"

// EXP-877: the ONE coding action in the issue tray, decided from the issue's
// runs (never from which face is showing):
//
//   my live run (running/in_review, heartbeat alive)   → Stop
//   my newest ended run, resumable on its machine      → Resume
//   otherwise                                          → Start coding
//
// A teammate's live run changes nothing here — the tray carries no caption
// about it any more; the synced status badge in the lists says it. On the
// session route `preferredSessionId` names the run the tab is bound to, so
// Stop/Resume act on THAT run when it qualifies, else on the newest own one.

function newest<T extends { startedAt: Date | string }>(rows: T[]): T | null {
  if (rows.length === 0) return null
  return rows.reduce((best, row) =>
    new Date(row.startedAt) > new Date(best.startedAt) ? row : best
  )
}

export function IssueCodingAction({
  issue,
  board,
  teamId,
  currentUserId,
  preferredSessionId,
  showStart = true,
}: {
  issue: Issue
  board: Board
  teamId: string
  currentUserId: string
  preferredSessionId?: string
  /** EXP-893: the phone's Run-face header carries Stop / Resume only — its
   *  bottom-right circle owns Start coding. `false` = no Start capsule. */
  showStart?: boolean
}) {
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) => eq(s.issueId, issue.id)),
    [issue.id]
  )
  const now = useNow()
  const own = useMemo(
    () =>
      ((sessionRows ?? []) as CodingSession[]).filter(
        (row) => row.userId === currentUserId
      ),
    [sessionRows, currentUserId]
  )
  const pick = (candidates: CodingSession[]) =>
    candidates.find((row) => row.id === preferredSessionId) ??
    newest(candidates)
  // Staleness guard (EXP-153): heartbeat-dead rows render as absent.
  const ownLive = useMemo(
    () =>
      pick(
        own.filter(
          (row) =>
            (row.status === `running` || row.status === `in_review`) &&
            !isCodingSessionStale(row.updatedAt, now)
        )
      ),
    [own, now, preferredSessionId]
  )
  // Resume only when the NEWEST own row is itself ended (or the bound run
  // is): a newer own run whose heartbeat died — still `running`, device
  // offline — is neither live nor resumable, and offering Resume on an older
  // run behind it would fork the issue's work. Falls through to Start coding.
  const ownEnded = useMemo(() => {
    const preferred = own.find(
      (row) => row.id === preferredSessionId && row.status === `ended`
    )
    if (preferred) return preferred
    const newestOwn = newest(own)
    return newestOwn?.status === `ended` ? newestOwn : null
  }, [own, preferredSessionId])
  const canResume = useCanResumeOn(ownLive ? null : ownEnded)

  if (ownLive) {
    return (
      <SessionStopRunPill session={ownLive} currentUserId={currentUserId} />
    )
  }
  if (ownEnded && canResume) {
    return <ResumeRunPill session={ownEnded} />
  }
  if (!showStart) return null
  return (
    <IssueCodingControl
      issue={issue}
      board={board}
      teamId={teamId}
      currentUserId={currentUserId}
      variant="start"
      // Two accent pills in one slot say nothing about which one to press:
      // Start coding steps down to glass while Merge is primary (EXP-760).
      tone={issue.prState === `open` ? `glass` : `primary`}
    />
  )
}
