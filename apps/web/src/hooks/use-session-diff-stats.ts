import { useEffect, useMemo, useSyncExternalStore } from "react"
import { parseDiff, totals } from "@exp/domain-contract/diff"
import { acquireSteerSession } from "@/lib/steer-session-store"

// EXP-893: whether (and how big) the run's LIVE diff is, read off the
// per-session steer store without mounting the session view — the phone's
// issue route needs it to know whether its Work screen has a Changes face
// before the reader switches to it. `acquireSteerSession` is get-or-create
// and never dials; the store only carries a diff once something connected.
//
// EXP-889: the issue face (every width) reads it too, and `connect` dials the
// store itself, so the diff item survives what used to empty it: a merge ENDS
// the run, its store self-disposes once the session view unmounts, and the
// counts only came back when the Run face redialled and replayed history.
// Subscribing here also keeps an ended store from self-disposing.

export interface SessionDiffStats {
  fileCount: number
  additions: number
  deletions: number
}

const EMPTY: SessionDiffStats = { fileCount: 0, additions: 0, deletions: 0 }

const noSubscribe = () => () => {}
const noDiff = () => null

export function useSessionDiffStats(
  sessionId: string | null | undefined,
  options: { connect?: boolean } = {}
): SessionDiffStats {
  const connect = options.connect ?? false
  const store = useMemo(
    () => (sessionId ? acquireSteerSession(sessionId) : null),
    [sessionId]
  )
  useEffect(() => {
    if (connect) store?.connect()
  }, [store, connect])
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
