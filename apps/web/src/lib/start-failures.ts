// FEED-63: the device's channel for "I took your start frame but could not
// launch the run" (a refused `git checkout`, a missing worktree...). Before
// this, `exponential_sessions_start` could only time out with a generic
// "reported no run within 10s" while the real reason sat in the daemon log.
//
// Flow: `steer.startSession` mints a `startId` right before it posts the
// frame to the relay (the relay forwards it in the `start_session` frame);
// a device that fails the launch calls `steer.reportStartFailure({ startId,
// reason })` as the device OWNER; the MCP start poll takes the reason and
// answers with it at once.
//
// Single-process and in-memory on purpose (same precedent as
// `integrations/pr-actor-claims.ts`): a miss (restart, another replica)
// degrades to the old timeout message, never a crash.

import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"

interface StartEntry {
  // Who may report a failure for this start: the device owner (the daemon
  // authenticates as them). Equal to `requesterId` on an own-device start.
  reporterId: string
  // Who may read it back: the user who asked for the start.
  requesterId: string
  failure: { reason: string; at: Date } | null
  expiresAt: number
}

const START_TTL_MS = 10 * 60 * 1000
const MAX_STARTS = 1000

const starts = new Map<string, StartEntry>()

function prune(now: number): void {
  for (const [id, entry] of starts) {
    if (entry.expiresAt <= now) starts.delete(id)
  }
}

/** Registers a start id. `userId` = the device owner (the only one a
 *  failure report is accepted from); `requesterId` = who reads the failure
 *  back (defaults to the owner; differs on a shared-device start).
 *  Review #3: a client-minted `startId` (one per Start press, REUSED by
 *  Retry so the device can drop a start it already ran) is registered as is;
 *  re-registering it for the same owner + requester refreshes its expiry and
 *  clears an old failure, while an id already held by anyone else gets a
 *  fresh one instead (never a takeover). Returns the id the frame carries. */
export function mintStartId(
  userId: string,
  requesterId: string = userId,
  startId?: string
): string {
  const now = Date.now()
  prune(now)
  let id = startId ?? crypto.randomUUID()
  const existing = starts.get(id)
  if (existing) {
    if (existing.reporterId === userId && existing.requesterId === requesterId) {
      // Re-insert so the refreshed id is the newest for the eviction order.
      starts.delete(id)
    } else {
      id = crypto.randomUUID()
    }
  }
  while (starts.size >= MAX_STARTS) {
    const oldest = starts.keys().next().value
    if (oldest === undefined) break
    starts.delete(oldest)
  }
  starts.set(id, {
    reporterId: userId,
    requesterId,
    failure: null,
    expiresAt: now + START_TTL_MS,
  })
  return id
}

/** Records the device's failure reason. False (never a throw) when the id is
 *  unknown, expired, or was minted for another user. */
export function recordStartFailure(input: {
  startId: string
  userId: string
  reason: string
}): boolean {
  const entry = starts.get(input.startId)
  if (!entry) return false
  if (entry.expiresAt <= Date.now()) {
    starts.delete(input.startId)
    return false
  }
  if (entry.reporterId !== input.userId) return false
  entry.failure = { reason: input.reason, at: new Date() }
  return true
}

/** Consumes a recorded failure. Null when none was recorded (yet), the id is
 *  unknown or expired, or `userId` did not ask for this start. A pending
 *  start (no failure yet) stays registered for a later report. */
export function takeStartFailure(
  startId: string,
  userId: string
): { reason: string; at: Date } | null {
  const entry = starts.get(startId)
  if (!entry) return null
  if (entry.expiresAt <= Date.now()) {
    starts.delete(startId)
    return null
  }
  if (entry.requesterId !== userId || !entry.failure) return null
  starts.delete(startId)
  return entry.failure
}

// Final review B8: the server's start dedupe (feedback-states.json
// `starting.dedupeMs`, 30 min). A client re-sends ONE draft start under the
// same `startId` (Retry, a later Start of the same draft); a start already
// forwarded for that (requester, device, startId) is not forwarded again and
// answers what the first one answered. Devices below the startId build
// ignore the id, so this is what keeps a Retry from starting a second run
// there. Only a SUCCESSFUL forward is remembered (a refusal or relay error
// re-forwards on Retry: the device's own dedupe covers a frame that did
// arrive); a repeat while the first is still in flight awaits it.
// Per-process like the registry above: another replica or a restart
// forwards again, and the device dedupe is the backstop.

const DEDUPE_MS = FEEDBACK_STATES.starting.dedupeMs

interface ForwardedStart {
  answer: Promise<unknown>
  // Null while in flight; set once the first forward succeeded.
  expiresAt: number | null
}

const forwarded = new Map<string, ForwardedStart>()

function pruneForwarded(now: number): void {
  for (const [key, entry] of forwarded) {
    if (entry.expiresAt !== null && entry.expiresAt <= now) {
      forwarded.delete(key)
    }
  }
}

/** Runs `forward` once per (requesterId, deviceId, startId) within
 *  `starting.dedupeMs`; a start without a client `startId` always runs. */
export function forwardStartOnce<T>(
  key: { requesterId: string; deviceId: string; startId?: string },
  forward: () => Promise<T>
): Promise<T> {
  if (!key.startId) return forward()
  const now = Date.now()
  pruneForwarded(now)
  const id = JSON.stringify([key.requesterId, key.deviceId, key.startId])
  const prior = forwarded.get(id)
  if (prior) return prior.answer as Promise<T>
  while (forwarded.size >= MAX_STARTS) {
    const oldest = forwarded.keys().next().value
    if (oldest === undefined) break
    forwarded.delete(oldest)
  }
  const entry: ForwardedStart = { answer: Promise.resolve(), expiresAt: null }
  const answer = forward().then(
    (result) => {
      entry.expiresAt = Date.now() + DEDUPE_MS
      return result
    },
    (error: unknown) => {
      if (forwarded.get(id) === entry) forwarded.delete(id)
      throw error
    }
  )
  entry.answer = answer
  forwarded.set(id, entry)
  return answer
}

// Test hook.
export function _clearStartFailures(): void {
  starts.clear()
  forwarded.clear()
}
