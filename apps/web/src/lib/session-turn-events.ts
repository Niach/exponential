import { useMemo } from "react"
import type { SessionTurnEvent } from "@exp/ui"
import type { TurnState } from "@/lib/agent-feed"
import { parseSteerMessage } from "@/lib/steer-image-message"

// EXP-1245: the facts the OWNER's thread of turns reads off the relay feed
// they already hold (`sessionTurns`, @exp/ui): their messages (`user_message`
// rows, main agent only) and the turn edges. The store keeps the turn as a
// latest-wins SLOT, not as rows, so this records every edge it OBSERVES into
// a per-session log that outlives the view (the store outlives it too,
// EXP-621): a `started` with its `startedAt`, an `ended` only when it is seen
// to follow a start (an `ended` slot found on arrival has no time to give).
// A message carries the relay's `at` when the store kept it; one without is
// stamped when it ARRIVES live (a lone new row), never a history replay's
// bulk (its times are unknown, so those rows stay out of the turns). The
// run's own start opens the first turn (its prompt is the issue or the
// composer's text, no bubble), so results published before any observed edge
// land there instead of in the newest turn.

interface FeedRow {
  id: number
  kind: string
  text?: string
  subagentId?: string
  at?: number
}

export interface TurnLog {
  /** Recorded edges, in observation order. */
  edges: Extract<SessionTurnEvent, { kind: `turn` }>[]
  /** Feed row id → the time it is placed at. */
  messageAt: Map<number, number>
  /** Every feed row id seen so far. */
  seen: Set<number>
  primed: boolean
  lastState: TurnState | null
  lastStart: number | null
}

/** A lone new row (or two: an echo and its twin) is a live arrival. */
export const LIVE_ARRIVAL_MAX_ROWS = 2

export function emptyTurnLog(): TurnLog {
  return {
    edges: [],
    messageAt: new Map(),
    seen: new Set(),
    primed: false,
    lastState: null,
    lastStart: null,
  }
}

/** Fold the turn slot's current value into the log. */
export function recordTurnSlot(
  log: TurnLog,
  state: TurnState,
  startedAt: number | null,
  now: number
): void {
  const start = startedAt !== null && Number.isFinite(startedAt) ? startedAt : null
  if (state === `started`) {
    const at = start ?? now
    if (log.lastState !== `started` || (start !== null && start !== log.lastStart)) {
      log.edges.push({ kind: `turn`, state: `started`, at })
    }
    log.lastStart = start ?? log.lastStart ?? at
  } else if (log.lastState === `started`) {
    log.edges.push({ kind: `turn`, state: `ended`, at: Math.max(now, log.lastStart ?? now) })
  }
  log.lastState = state
}

/** Fold the feed's rows into the log: place every main-agent message. */
export function recordFeedMessages(
  log: TurnLog,
  feed: readonly FeedRow[],
  now: number
): void {
  const fresh = feed.filter((row) => !log.seen.has(row.id))
  const live = log.primed && fresh.length <= LIVE_ARRIVAL_MAX_ROWS
  for (const row of fresh) {
    log.seen.add(row.id)
    if (row.kind !== `user_message` || row.subagentId) continue
    if (typeof row.at === `number` && Number.isFinite(row.at)) log.messageAt.set(row.id, row.at)
    else if (live) log.messageAt.set(row.id, now)
  }
  log.primed = true
}

/** The events `sessionTurns` walks: the run's start, the placed messages
 *  (still in the feed) and the recorded edges. Empty when nothing was
 *  observed beyond the start (the single-row thread). */
export function turnEventsOf(
  log: TurnLog,
  feed: readonly FeedRow[],
  runStartedAt: number | null
): SessionTurnEvent[] {
  const messages: SessionTurnEvent[] = []
  for (const row of feed) {
    const at = log.messageAt.get(row.id)
    if (at === undefined || row.kind !== `user_message` || !row.text) continue
    const parsed = parseSteerMessage(row.text)
    if (!parsed.text && parsed.attachmentIds.length === 0) continue
    messages.push({
      kind: `user_message`,
      at,
      text: parsed.text,
      images: parsed.attachmentIds.map((id) => `/api/attachments/${id}`),
    })
  }
  if (messages.length === 0 && log.edges.length === 0) return []
  const first =
    runStartedAt !== null && Number.isFinite(runStartedAt)
      ? [{ kind: `turn` as const, state: `started` as const, at: runStartedAt }]
      : []
  return [...first, ...messages, ...log.edges]
}

const logs = new Map<string, TurnLog>()

export function turnLogFor(sessionId: string): TurnLog {
  let log = logs.get(sessionId)
  if (!log) {
    log = emptyTurnLog()
    logs.set(sessionId, log)
  }
  return log
}

/** The owner's turn events for `sessionTurns` (see above). */
export function useSessionTurnEvents(
  sessionId: string,
  feed: readonly FeedRow[],
  turnState: TurnState,
  turnStartedAt: number | null,
  runStartedAt: Date | string | number | null | undefined
): SessionTurnEvent[] {
  const startMs =
    runStartedAt === null || runStartedAt === undefined
      ? null
      : new Date(runStartedAt).getTime()
  return useMemo(() => {
    const log = turnLogFor(sessionId)
    const now = Date.now()
    recordTurnSlot(log, turnState, turnStartedAt, now)
    recordFeedMessages(log, feed, now)
    return turnEventsOf(log, feed, Number.isFinite(startMs) ? startMs : null)
  }, [sessionId, feed, turnState, turnStartedAt, startMs])
}
