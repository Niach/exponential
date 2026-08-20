// Abandoned device-code sweep (EXP-403). The Better Auth deviceAuthorization
// plugin deletes a `device_codes` row when its grant resolves (token issued,
// denied, or an EXPIRED POLL) — but a flow the CLI abandons without ever
// polling again leaves the row behind forever, and /device/code is an
// unauthenticated endpoint, so the leak is attacker-growable. This sweep
// bounds it: rows a full day past their (10-minute) expiry serve no purpose —
// any late poll would be answered `expired_token` and delete the row anyway.
// Mirrors fcm-token-sweep.ts's in-process scheduler shell; started once from
// server-bun.ts. Multi-instance safe: the delete is idempotent.

import { lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { deviceCodes } from "@/db/schema"
import { reportSchedulerRun } from "@/lib/metrics/registry"

export const DEVICE_CODE_RETENTION_MS = 24 * 60 * 60 * 1000

const INITIAL_DELAY_MS = 4 * 60 * 1000
const SWEEP_INTERVAL_MS = 60 * 60 * 1000

// One sweep pass, injectable clock for tests/manual runs.
export async function runDeviceCodeSweep(
  now: Date = new Date()
): Promise<{ codesDeleted: number }> {
  const cutoff = new Date(now.getTime() - DEVICE_CODE_RETENTION_MS)
  const deleted = await db
    .delete(deviceCodes)
    .where(lte(deviceCodes.expiresAt, cutoff))
    .returning({ id: deviceCodes.id })
  return { codesDeleted: deleted.length }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    const result = await runDeviceCodeSweep()
    reportSchedulerRun(`device-code-sweep`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.codesDeleted} deleted`,
    })
    if (result.codesDeleted > 0) {
      console.log(
        `[device-code-sweep] deleted ${result.codesDeleted} abandoned device-code row(s)`
      )
    }
  } catch (err) {
    reportSchedulerRun(`device-code-sweep`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[device-code-sweep] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process sweep scheduler — call once at boot (server-bun.ts).
// Double-start-guarded within the process.
export function startDeviceCodeSweepScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
