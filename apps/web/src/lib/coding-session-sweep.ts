// Coding-session staleness sweep. A coding_sessions row is normally flipped to
// `ended` by the desktop's in-process exit hook (or a manual steer kill), but a
// desktop SIGKILL/panic/power loss fires neither and nothing reconciles on
// relaunch — the row would stay `running` forever, pinning a phantom
// "coding now" badge on every client and letting boards keep minting steer
// view tickets for a dead relay room.
//
// EXP-888: the sweep ENDS a stale row with `ended_by = stale`, so the run keeps
// its place in every runs list and its on-device transcript stays openable and
// resumable for as long as the device keeps it (EXP-886: journals are pruned
// per the user's `sessionRetentionDays`,
// apps/desktop/crates/coding/src/session_retention.rs). `stale` is the one end
// that never kills: the desktop kill-watch and the CLI kill-poll ignore it
// (the run may just be a laptop that slept past the window), and the next
// heartbeat of a run that turns out to be alive REVIVES the row
// (codingSessions.heartbeat), restoring badge + steerability.
//
// The flip is gated on the hosting device advertising the `stale-end` cap
// (every build past the version floor does; the cap stays as the contract
// the device signs). A stale row whose device row still exists is only ever
// FLIPPED through that path — never deleted: a delete would drop the run
// from every list and orphan its journal. The DELETE is confined to stale
// rows with NO devices row to consult at all — device-less legacy rows and
// orphans left by `devices.remove` (it drops the devices row without
// touching sessions) — which nothing could ever flip or revive (EXP-972).
// Staleness is measured from updated_at, which the heartbeat advances, so a
// heartbeating session never goes stale.
//
// Mirrors board-trash.ts's in-process scheduler shell; started once from
// server-bun.ts. Multi-instance safe by construction: both statements are
// status-conditioned atomic claims over DISJOINT rows (a devices row exists
// or it does not), and the desktop's own end tolerates either.

import { and, inArray, lte, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, devices } from "@/db/schema"
import { CODING_SESSION_STALE_MS } from "@exp/db-schema/domain"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { notifyParentOfChildEnd } from "@/lib/steer-child-messages"

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
  // No devices row at all for the run's host + device: a legacy row that
  // never named a device, or one whose device was removed (EXP-972).
  const hostDeviceGone = sql`not exists (
    select 1 from ${devices}
    where ${devices.userId} = coalesce(${codingSessions.hostUserId}, ${codingSessions.userId})
      and ${devices.deviceId} = ${codingSessions.deviceId}
  )`

  // A failure here (a lock timeout, the `caps ?` operator on a drifted
  // devices row) must not abort the whole pass: the orphan delete below still
  // runs, and the rows this flip missed simply wait for the next pass — they
  // are never deleted in its place.
  let ended: { id: string; parentSessionId: string | null }[] = []
  try {
    ended = await db
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
      .returning({
        id: codingSessions.id,
        parentSessionId: codingSessions.parentSessionId,
      })
  } catch (err) {
    console.error(
      `[coding-session-sweep] stale-end flip failed, retrying next pass:`,
      err
    )
  }

  const deleted = await db
    .delete(codingSessions)
    .where(and(stale, hostDeviceGone))
    .returning({ id: codingSessions.id })

  // EXP-700: a swept row may be an agent-started CHILD whose parent is blocked
  // waiting for a report it will now never send — the child's `sessions_end`
  // never ran, which is precisely why the sweep saw it go silent. Tell the
  // parent, exactly like the client/merge end paths do. `notifyParentOfChildEnd`
  // no-ops for every row that is not agent-started with a live linked parent
  // and never throws, so this stays best-effort; the parentSessionId filter
  // just keeps a big sweep from looking up rows that can't qualify.
  await Promise.all(
    ended
      .filter((row) => row.parentSessionId)
      .map((row) =>
        notifyParentOfChildEnd(db, row.id, {
          summary: null,
          endedBy: `stale`,
        })
      )
  )

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
