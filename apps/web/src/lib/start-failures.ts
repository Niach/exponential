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

/** Registers a fresh start id. `userId` = the device owner (the only one a
 *  failure report is accepted from); `requesterId` = who reads the failure
 *  back (defaults to the owner; differs on a shared-device start). */
export function mintStartId(userId: string, requesterId: string = userId): string {
  const now = Date.now()
  prune(now)
  while (starts.size >= MAX_STARTS) {
    const oldest = starts.keys().next().value
    if (oldest === undefined) break
    starts.delete(oldest)
  }
  const startId = crypto.randomUUID()
  starts.set(startId, {
    reporterId: userId,
    requesterId,
    failure: null,
    expiresAt: now + START_TTL_MS,
  })
  return startId
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

// Test hook.
export function _clearStartFailures(): void {
  starts.clear()
}
