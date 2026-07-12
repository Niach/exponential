// Stale FCM-token sweep. register() upserts per (token, user) and every
// healthy client re-registers all of its signed-in accounts on launch, bumping
// updated_at. A row that stops being refreshed belongs to an account that no
// longer runs on that device: a sign-out whose best-effort unregister never
// reached the server (offline, timeout, server error) or an already-shipped
// client build that never calls unregister at all. Since register() no longer
// steals the token row from other users, nothing else removes such rows — the
// device would keep receiving the departed account's notification content
// forever. This sweep bounds that leak. Mirrors project-trash.ts's in-process
// scheduler shell; started once from server-bun.ts. Multi-instance safe: the
// delete is idempotent.

import { lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { fcmTokens } from "@/db/schema"

// Firebase's own guidance treats tokens unrefreshed for about a month as
// stale. A signed-in account that never opens the app for this long loses
// pushes until the next launch re-registers it — the deliberate cost of
// bounding how long a signed-out account's row can keep leaking pushes.
export const FCM_TOKEN_STALE_MS = 30 * 24 * 60 * 60 * 1000

const INITIAL_DELAY_MS = 3 * 60 * 1000
const SWEEP_INTERVAL_MS = 6 * 60 * 60 * 1000

// Pure staleness predicate: a token row is due for deletion once its last
// re-registration is a full staleness window in the past. The sweep query
// applies the equivalent cutoff server-side; this documents (and tests) it.
export function isFcmTokenStale(
  updatedAt: Date,
  now: Date = new Date()
): boolean {
  return updatedAt.getTime() + FCM_TOKEN_STALE_MS <= now.getTime()
}

// One sweep pass, injectable clock for tests/manual runs.
export async function runFcmTokenSweep(
  now: Date = new Date()
): Promise<{ tokensDeleted: number }> {
  const cutoff = new Date(now.getTime() - FCM_TOKEN_STALE_MS)
  const deleted = await db
    .delete(fcmTokens)
    .where(lte(fcmTokens.updatedAt, cutoff))
    .returning({ id: fcmTokens.id })
  return { tokensDeleted: deleted.length }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  try {
    const result = await runFcmTokenSweep()
    if (result.tokensDeleted > 0) {
      console.log(
        `[fcm-token-sweep] deleted ${result.tokensDeleted} stale token row(s)`
      )
    }
  } catch (err) {
    console.error(`[fcm-token-sweep] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process sweep scheduler — call once at boot (server-bun.ts).
// Double-start-guarded within the process.
export function startFcmTokenSweepScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
