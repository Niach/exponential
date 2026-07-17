// Coding-session staleness sweep. A coding_sessions row is normally flipped to
// `ended` by the desktop's in-process exit hook (or a manual steer kill), but a
// desktop SIGKILL/panic/power loss fires neither and nothing reconciles on
// relaunch — the row would stay `running` forever, pinning a phantom
// "coding now" badge on every client and letting public boards keep minting
// steer view tickets for a dead relay room.
//
// The sweep DELETES stale rows instead of flipping them to `ended`: the
// desktop's own-row kill-switch (sync::kill_watch) treats the running→ended
// transition as a remote kill and tears the live claude child down, while a
// vanished row deliberately does NOT fire it — so deletion is the only sweep
// primitive that is badge-only by construction (the synced row IS the badge)
// and can never kill a session that turns out to be alive. The cost is that a
// crashed session leaves no "recently ended" recap entry, which would have
// carried a fabricated endedAt anyway. Two belts against sweeping live work:
// the desktop advances updated_at via codingSessions.heartbeat while the
// child runs (staleness is measured from updated_at, so a heartbeating
// session never goes stale however long it lives), and even a session whose
// heartbeats all failed only loses its badge/steerability, never its process.
//
// Mirrors project-trash.ts's in-process scheduler shell; started once from
// server-bun.ts. Multi-instance safe by construction: the status-conditioned
// DELETE is the atomic claim, and the desktop's own end mutation tolerates a
// vanished row either way.

import { and, eq, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions } from "@/db/schema"
import { CODING_SESSION_STALE_MS } from "@exp/db-schema/domain"

const INITIAL_DELAY_MS = 2 * 60 * 1000
const SWEEP_INTERVAL_MS = 30 * 60 * 1000

// The pure staleness predicate lives in @exp/db-schema/domain
// (isCodingSessionStale) — it doubles as the client-side render guard on all
// four clients (EXP-153). The sweep query below applies the equivalent cutoff
// server-side.

// One sweep pass, injectable clock for tests/manual runs. Returns the count
// for the caller's logging.
export async function runCodingSessionSweep(
  now: Date = new Date()
): Promise<{ sessionsDeleted: number }> {
  const cutoff = new Date(now.getTime() - CODING_SESSION_STALE_MS)

  const deleted = await db
    .delete(codingSessions)
    .where(
      and(
        eq(codingSessions.status, `running`),
        lte(codingSessions.updatedAt, cutoff)
      )
    )
    .returning({ id: codingSessions.id })

  return { sessionsDeleted: deleted.length }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  try {
    const result = await runCodingSessionSweep()
    if (result.sessionsDeleted > 0) {
      console.log(
        `[coding-session-sweep] deleted ${result.sessionsDeleted} stale running session(s)`
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
