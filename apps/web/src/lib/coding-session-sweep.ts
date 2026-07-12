// Coding-session staleness sweep. A coding_sessions row is normally flipped to
// `ended` by the desktop's in-process exit hook (or a manual steer kill), but a
// desktop SIGKILL/panic/power loss fires neither and nothing reconciles on
// relaunch — the row would stay `running` forever, pinning a phantom
// "coding now" badge on every client and letting public boards keep minting
// steer view tickets for a dead relay room. This sweep force-ends rows still
// `running` past CODING_SESSION_STALE_MS. Mirrors project-trash.ts's
// in-process scheduler shell; started once from server-bun.ts. Multi-instance
// safe by construction: the status-conditioned UPDATE is the atomic claim, and
// the desktop's own end mutation is idempotent either way.

import { and, eq, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions } from "@/db/schema"
import { CODING_SESSION_STALE_MS } from "@exp/db-schema/domain"

const INITIAL_DELAY_MS = 2 * 60 * 1000
const SWEEP_INTERVAL_MS = 30 * 60 * 1000

// Pure staleness predicate: a running session is stale once startedAt plus the
// staleness window has passed. The sweep query applies the equivalent cutoff
// server-side; this documents (and tests) the rule.
export function isCodingSessionStale(
  startedAt: Date,
  now: Date = new Date()
): boolean {
  return startedAt.getTime() + CODING_SESSION_STALE_MS <= now.getTime()
}

// One sweep pass, injectable clock for tests/manual runs. Returns the count
// for the caller's logging.
export async function runCodingSessionSweep(
  now: Date = new Date()
): Promise<{ sessionsEnded: number }> {
  const cutoff = new Date(now.getTime() - CODING_SESSION_STALE_MS)

  const ended = await db
    .update(codingSessions)
    .set({ status: `ended`, endedAt: now })
    .where(
      and(
        eq(codingSessions.status, `running`),
        lte(codingSessions.startedAt, cutoff)
      )
    )
    .returning({ id: codingSessions.id })

  return { sessionsEnded: ended.length }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  try {
    const result = await runCodingSessionSweep()
    if (result.sessionsEnded > 0) {
      console.log(
        `[coding-session-sweep] force-ended ${result.sessionsEnded} stale running session(s)`
      )
    }
  } catch (err) {
    console.error(`[coding-session-sweep] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process staleness scheduler — call once at boot (server-bun.ts).
// Double-start-guarded within the process. Worst-case a stale row lives
// ~SWEEP_INTERVAL_MS past its staleness window, which is fine for a badge.
export function startCodingSessionSweepScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
