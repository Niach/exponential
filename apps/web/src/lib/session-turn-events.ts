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
// FIRST non-empty feed the log sees is always a replay (the store joined
// with history, however short), so nothing in it is stamped: rows count as
// live only after that, and only when they carry ids above every row seen
// (a history page prepends lower ids). The run's own start opens the first
// turn (its prompt is the issue or the composer's text, no bubble), so
// results published before any observed edge land there instead of in the
// newest turn. The per-session log is released with the session's feed
// store (`releaseTurnLog`) and keeps a bounded `seen` set.

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
  /** Every feed row id seen so far (the newest `SEEN_CAP`). */
  seen: Set<number>
  /** The highest row id seen so far; a live arrival has a higher one. */
  maxSeen: number
  /** A non-empty feed was folded in: later lone rows are live arrivals. */
  primed: boolean
  lastState: TurnState | null
  lastStart: number | null
}

/** A lone new row (or two: an echo and its twin) is a live arrival. */
export const LIVE_ARRIVAL_MAX_ROWS = 2

/** The `seen` set keeps this many ids (the newest); the store's feed is
 *  itself capped well below it, so a trimmed row never reads as fresh. */
export const SEEN_CAP = 4_096

export function emptyTurnLog(): TurnLog {
  return {
    edges: [],
    messageAt: new Map(),
    seen: new Set(),
    maxSeen: -1,
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
  // Live = the log already folded a non-empty feed (so this is not the
  // replay the view mounted on), a lone row or two, every one NEWER than
  // anything seen (a history page prepends older ids).
  const live =
    log.primed &&
    fresh.length <= LIVE_ARRIVAL_MAX_ROWS &&
    fresh.every((row) => row.id > log.maxSeen)
  for (const row of fresh) {
    log.seen.add(row.id)
    if (row.id > log.maxSeen) log.maxSeen = row.id
    if (row.kind !== `user_message` || row.subagentId) continue
    if (typeof row.at === `number` && Number.isFinite(row.at)) log.messageAt.set(row.id, row.at)
    else if (live) log.messageAt.set(row.id, now)
  }
  if (feed.length > 0) log.primed = true
  trimSeen(log)
}

/** Keep `seen` (and the placements of rows the feed can no longer hold)
 *  bounded: drop the oldest ids past `SEEN_CAP`. Ids only grow, so a
 *  dropped id never comes back as a fresh row. */
function trimSeen(log: TurnLog): void {
  if (log.seen.size <= SEEN_CAP) return
  const drop = log.seen.size - SEEN_CAP
  let dropped = 0
  for (const id of log.seen) {
    if (dropped >= drop) break
    log.seen.delete(id)
    log.messageAt.delete(id)
    dropped++
  }
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
    if (
      !parsed.text &&
      parsed.attachmentIds.length === 0 &&
      parsed.files.length === 0
    )
      continue
    messages.push({
      kind: `user_message`,
      at,
      text: parsed.text,
      images: parsed.attachmentIds.map((id) => `/api/attachments/${id}`),
      ...(parsed.files.length > 0
        ? {
            files: parsed.files.map((file) => ({
              name: file.name,
              url: `/api/attachments/${file.id}`,
            })),
          }
        : {}),
    })
  }
  if (messages.length === 0 && log.edges.length === 0) return []
  const first =
    runStartedAt !== null && Number.isFinite(runStartedAt)
      ? [{ kind: `turn` as const, state: `started` as const, at: runStartedAt }]
      : []
  return [...first, ...messages, ...log.edges]
}

/** Whether the FIRST turn's end is a real observation. Its start is the
 *  run's own (synthetic), so its end is known only when the view watched it
 *  run: the first event after the run's start is an observed `started`
 *  edge. Mounting after it ended leaves only the next message to close it,
 *  and that time includes the idle gap. */
export function firstTurnEndKnown(
  events: readonly SessionTurnEvent[],
  runStartedAt: Date | string | number | null | undefined
): boolean {
  const start =
    runStartedAt === null || runStartedAt === undefined
      ? Number.NaN
      : new Date(runStartedAt).getTime()
  const [first, ...rest] = events
  // No synthetic run start in front: every turn opened on an observed edge.
  if (!first || first.kind !== `turn` || first.state !== `started` || first.at !== start) {
    return true
  }
  const next = rest
    .map((event, order) => ({ event, order }))
    .filter(({ event }) => Number.isFinite(event.at))
    .sort((a, b) => a.event.at - b.event.at || a.order - b.order)[0]?.event
  return next === undefined || (next.kind === `turn` && next.state === `started`)
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

/** Drop a session's log: called when its feed store is disposed (the feed
 *  it was folded from is gone, so the next view starts over). */
export function releaseTurnLog(sessionId: string): void {
  logs.delete(sessionId)
}

/** Whether a log is held for the session (tests, diagnostics). */
export function hasTurnLog(sessionId: string): boolean {
  return logs.has(sessionId)
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
