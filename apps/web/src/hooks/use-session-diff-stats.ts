import { useEffect, useMemo, useSyncExternalStore } from "react"
import { parseDiff, totals } from "@exp/domain-contract/diff"
import type { CodingSession } from "@/db/schema"
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
//
// EXP-875: `connect` is NOT "always" — see `shouldConnectSessionDiff`. A dial
// mints a relay ticket and, with no live room, asks the device to republish
// its journal; doing that for every ended run an issue ever had, just to print
// `+N −M`, is a lot of machine for a caption.

export interface SessionDiffStats {
  fileCount: number
  additions: number
  deletions: number
}

const EMPTY: SessionDiffStats = { fileCount: 0, additions: 0, deletions: 0 }

const noSubscribe = () => () => {}
const noDiff = () => null

/** EXP-875: how long after a merge the issue page still dials for the run's
 *  diff. A merge ENDS the run, and EXP-889 wants the counts to survive that —
 *  but only for the reader who was there, not for anyone opening the issue
 *  next week. */
export const MERGED_DIFF_WINDOW_MS = 10 * 60 * 1000

export interface DiffConnectSession {
  status: CodingSession[`status`]
  prState?: CodingSession[`prState`] | null
  /** Accepts the synced row's `Date` or a wire string either way. */
  endedAt?: Date | string | null
  updatedAt?: Date | string | null
}

/**
 * Whether a surface that only wants the NUMBERS may open a relay socket for
 * this run. Dialing costs a minted ticket and, with no live room, makes the
 * device republish its whole journal — far too much for a `+N −M` caption on
 * a run that finished long ago (EXP-875). Yes for a live run, for one whose
 * PR is still open, and briefly after a merge; never while steering is off.
 */
export function shouldConnectSessionDiff(input: {
  session: DiffConnectSession | null | undefined
  /** The issue's PR state — a run that opened no PR of its own rides it. */
  issuePrState?: CodingSession[`prState`] | null
  steerEnabled: boolean
  now: Date
}): boolean {
  const { session, steerEnabled, now } = input
  if (!session || !steerEnabled) return false
  if (session.status === `running` || session.status === `in_review`) return true
  const prState = session.prState ?? input.issuePrState ?? null
  if (prState === `open`) return true
  if (prState !== `merged`) return false
  const ended = session.endedAt ?? session.updatedAt ?? null
  const stamp = ended ? new Date(ended).getTime() : NaN
  if (Number.isNaN(stamp)) return false
  return now.getTime() - stamp <= MERGED_DIFF_WINDOW_MS
}

export function useSessionDiffStats(
  sessionId: string | null | undefined,
  options: {
    connect?: boolean
    /** The run's synced status — the truth for "still running" inside the
     *  store's redial loops, so a wakeup (`visibilitychange`/`online`) never
     *  re-dials an ended run. */
    status?: CodingSession[`status`] | null
  } = {}
): SessionDiffStats {
  const connect = options.connect ?? false
  const status = options.status ?? null
  const store = useMemo(
    () => (sessionId ? acquireSteerSession(sessionId) : null),
    [sessionId]
  )
  // Before any dial: the store must know what it is looking at.
  useEffect(() => {
    if (store && status) store.noteSessionStatus(status)
  }, [store, status])
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
