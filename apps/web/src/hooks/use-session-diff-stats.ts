import { useMemo, useSyncExternalStore } from "react"
import { parseDiff, totals } from "@exp/domain-contract/diff"
import { acquireSteerSession } from "@/lib/steer-session-store"

// EXP-893: whether (and how big) the run's LIVE diff is, read off the
// per-session steer store without mounting the session view — the phone's
// issue route needs it to know whether its Work screen has a Changes face
// before the reader switches to it. `acquireSteerSession` is get-or-create
// and never dials; the store only carries a diff once something connected.

export interface SessionDiffStats {
  fileCount: number
  additions: number
  deletions: number
}

const EMPTY: SessionDiffStats = { fileCount: 0, additions: 0, deletions: 0 }

const noSubscribe = () => () => {}
const noDiff = () => null

export function useSessionDiffStats(
  sessionId: string | null | undefined
): SessionDiffStats {
  const store = useMemo(
    () => (sessionId ? acquireSteerSession(sessionId) : null),
    [sessionId]
  )
  const latestDiff = useSyncExternalStore(
    store ? store.subscribe : noSubscribe,
    store ? () => store.getSnapshot().latestDiff : noDiff
  )
  return useMemo(() => {
    if (!latestDiff) return EMPTY
    const sum = totals(parseDiff(latestDiff).files)
    return {
      fileCount: sum.files,
      additions: sum.additions,
      deletions: sum.deletions,
    }
  }, [latestDiff])
}
