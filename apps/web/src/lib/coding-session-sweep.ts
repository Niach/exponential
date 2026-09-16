// Coding-session staleness sweep. A coding_sessions row is normally flipped to
// `ended` by the desktop's in-process exit hook (or a manual steer kill), but a
// desktop SIGKILL/panic/power loss fires neither and nothing reconciles on
// relaunch — the row would stay `running` forever, pinning a phantom
// "coding now" badge on every client and letting boards keep minting steer
// view tickets for a dead relay room.
//
// EXP-888: the sweep ENDS a stale row with `ended_by = stale`, so the run keeps
// its place in every runs list and its on-device transcript (EXP-886 keeps
// journals indefinitely) stays openable and resumable. `stale` is the one end
// that never kills: the desktop kill-watch and the CLI kill-poll ignore it
// (the run may just be a laptop that slept past the window), and the next
// heartbeat of a run that turns out to be alive REVIVES the row
// (codingSessions.heartbeat), restoring badge + steerability.
//
// Old desktop/CLI builds read ANY running→ended flip as a remote kill, so the
// flip is gated on the hosting device advertising the `stale-end` cap; a row
// whose device lacks it (an older build, a device-less legacy row) is still
// DELETED as before — a vanished row never fires their kill-switch, and their
// heartbeat re-creates it. Staleness is measured from updated_at, which the
// heartbeat advances, so a heartbeating session never goes stale.
//
// Mirrors board-trash.ts's in-process scheduler shell; started once from
// server-bun.ts. Multi-instance safe by construction: both statements are
// status-conditioned atomic claims (the flip runs first, so the delete never
// sees a row it just ended), and the desktop's own end tolerates either.

import { and, inArray, lte, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, devices } from "@/db/schema"
import { CODING_SESSION_STALE_MS } from "@exp/db-schema/domain"
import { reportSchedulerRun } from "@/lib/metrics/registry"

const INITIAL_DELAY_MS = 2 * 60 * 1000
const SWEEP_INTERVAL_MS = 30 * 60 * 1000

// The pure staleness predicate lives in @exp/db-schema/domain
// (isCodingSessionStale) — it doubles as the client-side render guard on all
// four clients (EXP-153). The sweep query below applies the equivalent cutoff
// server-side.

// EXP-888: the device cap saying "this build ignores an `ended_by = stale`
// flip" (apps/desktop/crates/coding/src/doctor.rs DEVICE_CAPS).
export const STALE_END_CAP = `stale-end`

// One sweep pass, injectable clock for tests/manual runs. Returns the counts
// for the caller's logging.
export async function runCodingSessionSweep(
  now: Date = new Date()
): Promise<{ sessionsEnded: number; sessionsDeleted: number }> {
  const cutoff = new Date(now.getTime() - CODING_SESSION_STALE_MS)
  // in_review rows heartbeat too (the run is still alive during review) — a
  // crashed desktop must not pin a phantom "ready for review" badge any more
  // than a "coding now" one.
  const stale = and(
    inArray(codingSessions.status, [`running`, `in_review`]),
    lte(codingSessions.updatedAt, cutoff)
  )

  // The device row is the HOST's (a shared-device run is requester-owned
  // with host_user_id = the device owner, EXP-432).
  const hostKnowsStaleEnd = sql`exists (
    select 1 from ${devices}
    where ${devices.userId} = coalesce(${codingSessions.hostUserId}, ${codingSessions.userId})
      and ${devices.deviceId} = ${codingSessions.deviceId}
      and ${devices.caps} ? ${STALE_END_CAP}
  )`

  const ended = await db
    .update(codingSessions)
    .set({
      status: `ended`,
      endedAt: now,
      endedBy: `stale`,
      needsInput: false,
      // EXP-848/850: an ended run is never busy and says nothing.
      agentBusy: false,
      agentCaption: null,
    })
    .where(and(stale, hostKnowsStaleEnd))
    .returning({ id: codingSessions.id })

  const deleted = await db
    .delete(codingSessions)
    .where(stale)
    .returning({ id: codingSessions.id })

  return { sessionsEnded: ended.length, sessionsDeleted: deleted.length }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    const result = await runCodingSessionSweep()
    reportSchedulerRun(`coding-session-sweep`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.sessionsEnded} ended, ${result.sessionsDeleted} deleted`,
    })
    if (result.sessionsEnded + result.sessionsDeleted > 0) {
      console.log(
        `[coding-session-sweep] ended ${result.sessionsEnded}, deleted ${result.sessionsDeleted} stale session(s)`
      )
    }
  } catch (err) {
    reportSchedulerRun(`coding-session-sweep`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
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
