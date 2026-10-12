// EXP-1284 (review T16): server-side placement for daemons that predate
// threads. `steer.startSession({ threadId })` puts the thread on the relay
// frame and a current device echoes it into `codingSessions.start`; an older
// one drops the unknown field and the session would land in its DEFAULT
// thread (the issue's open one, or a fresh one), silently. So the start
// remembers the thread it validated, keyed by who owns the device, which
// device and the start's subject, with the frame's `startId`, and
// `codingSessions.start` takes it back when the device's start arrives
// WITHOUT a `threadId` (logged). A device that echoes the frame's `startId`
// gets EXACTLY its own entry (two starts of one issue on one device never
// swap threads); one that does not (an old daemon) gets the oldest pending
// entry of that subject, first in first out.
//
// Single-process and in-memory on purpose, like `start-failures.ts`: a miss
// (restart, another replica) degrades to the default placement, never a
// crash; MCP `sessions_start{threadId}` keeps its own fallback
// (`joinStartedRun`).

export type StartThreadSubject =
  | { issueId: string }
  | { issueIds: readonly string[] }
  | { actionId: string }

interface PendingStartThread {
  threadId: string
  subject: string
  /** The relay frame's start id (the client's, or the one the server minted). */
  startId: string
  expiresAt: number
}

const PENDING_TTL_MS = 5 * 60 * 1000
const MAX_PENDING = 1000

const pending = new Map<string, PendingStartThread[]>()

function deviceKey(ownerId: string, deviceId: string): string {
  return `${ownerId}\u0000${deviceId}`
}

/** One string per subject: an issue, a batch (its SET of issues: the device
 *  may echo them in any order) or an action (builtins by their literal). */
export function startThreadSubjectKey(subject: StartThreadSubject): string {
  if (`issueId` in subject) return `issue:${subject.issueId}`
  if (`issueIds` in subject) return `batch:${[...new Set(subject.issueIds)].sort().join(`,`)}`
  return `action:${subject.actionId}`
}

function prune(now: number): void {
  let total = 0
  for (const [key, list] of pending) {
    const live = list.filter((entry) => entry.expiresAt > now)
    if (live.length) {
      pending.set(key, live)
      total += live.length
    } else {
      pending.delete(key)
    }
  }
  // Oldest device keys go first past the cap (a flood never grows memory).
  for (const key of pending.keys()) {
    if (total <= MAX_PENDING) break
    total -= pending.get(key)!.length
    pending.delete(key)
  }
}

/** Called once the relay accepted a start frame that names a thread. Every
 *  start is its own entry (FIFO per device), never overwriting another; the
 *  same `startId` again (a deduped repeat) is a no-op. */
export function rememberStartThread(input: {
  ownerId: string
  deviceId: string
  subject: StartThreadSubject
  threadId: string
  startId: string
}): void {
  const now = Date.now()
  prune(now)
  const key = deviceKey(input.ownerId, input.deviceId)
  const list = pending.get(key) ?? []
  if (list.some((entry) => entry.startId === input.startId)) return
  list.push({
    threadId: input.threadId,
    subject: startThreadSubjectKey(input.subject),
    startId: input.startId,
    expiresAt: now + PENDING_TTL_MS,
  })
  pending.set(key, list)
}

/** Consumes the remembered thread of this device's start. `startId` = the
 *  frame's id the device echoed: EXACTLY that start's entry, else null (never
 *  another start's). Without one (an older daemon): the OLDEST pending entry
 *  of the subject. Null when none is pending. Called on EVERY non-resume
 *  start, so a device that does echo the thread clears its entry too. */
export function takeStartThread(input: {
  ownerId: string
  deviceId: string | null | undefined
  subject: StartThreadSubject
  startId?: string | null
}): string | null {
  if (!input.deviceId) return null
  const now = Date.now()
  const key = deviceKey(input.ownerId, input.deviceId)
  const list = pending.get(key)
  if (!list) return null
  const subject = startThreadSubjectKey(input.subject)
  const index = list.findIndex(
    (entry) =>
      entry.expiresAt > now &&
      entry.subject === subject &&
      (!input.startId || entry.startId === input.startId)
  )
  if (index === -1) return null
  const [entry] = list.splice(index, 1)
  if (!list.length) pending.delete(key)
  return entry!.threadId
}

// Test hook.
export function _clearStartThreads(): void {
  pending.clear()
}
