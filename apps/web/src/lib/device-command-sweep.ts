// Terminal device-command sweep. `device_commands` rows are minted by the web
// (a remote agent login, an OAuth code relay, an `update_now`) and claimed by
// the owning device's heartbeat, which calls devices.completeCommand to stamp
// the row `done` or `failed`. Nothing then removes it: a terminal row is pure
// history that no client reads back (getCommand serves the pending pickup,
// the completion result is surfaced live), so the table grows for the life of
// the instance and drags the partial pending index along with it. A `pending`
// row is NEVER swept, however old — a device that has been offline for weeks
// must still pick its command up on the next heartbeat.
//
// Mirrors device-code-sweep.ts's in-process scheduler shell; started once from
// server-bun.ts. Multi-instance safe: the delete is an idempotent,
// status-conditioned claim, and batched so one pass can never hold a lock on
// an unbounded row set.

import { and, inArray, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { deviceCommands } from "@/db/schema"
import { reportSchedulerRun } from "@/lib/metrics/registry"

// The states devices.completeCommand can leave behind — the only ones a
// device will never act on again. Keep in lockstep with that mutation.
export const DEVICE_COMMAND_TERMINAL_STATUSES = [`done`, `failed`] as const

export const DEVICE_COMMAND_RETENTION_MS = 7 * 24 * 60 * 60 * 1000

const INITIAL_DELAY_MS = 5 * 60 * 1000
const SWEEP_INTERVAL_MS = 60 * 60 * 1000

// Bounds one delete statement — a backlog just drains over the following
// passes rather than locking the whole table in one go.
const MAX_BATCH = 500

// Pure purge-due predicate: only a row in a TERMINAL state that has aged past
// the retention window is due. The sweep query applies the equivalent cutoff
// server-side; this documents (and tests) the rule.
export function isDeviceCommandPurgeDue(
  status: string,
  updatedAt: Date,
  now: Date = new Date()
): boolean {
  if (
    !(DEVICE_COMMAND_TERMINAL_STATUSES as readonly string[]).includes(status)
  ) {
    return false
  }
  return updatedAt.getTime() + DEVICE_COMMAND_RETENTION_MS <= now.getTime()
}

// One sweep pass, injectable clock for tests/manual runs. Returns the count
// for the caller's logging.
export async function runDeviceCommandSweep(
  now: Date = new Date()
): Promise<{ commandsDeleted: number }> {
  const cutoff = new Date(now.getTime() - DEVICE_COMMAND_RETENTION_MS)

  const due = await db
    .select({ id: deviceCommands.id })
    .from(deviceCommands)
    .where(
      and(
        inArray(deviceCommands.status, [...DEVICE_COMMAND_TERMINAL_STATUSES]),
        lte(deviceCommands.updatedAt, cutoff)
      )
    )
    .limit(MAX_BATCH)

  if (due.length === 0) return { commandsDeleted: 0 }

  // Re-applies the status + cutoff predicate so a row that stopped being due
  // between the select and the delete can never be lost.
  const deleted = await db
    .delete(deviceCommands)
    .where(
      and(
        inArray(
          deviceCommands.id,
          due.map((row) => row.id)
        ),
        inArray(deviceCommands.status, [...DEVICE_COMMAND_TERMINAL_STATUSES]),
        lte(deviceCommands.updatedAt, cutoff)
      )
    )
    .returning({ id: deviceCommands.id })

  return { commandsDeleted: deleted.length }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    const result = await runDeviceCommandSweep()
    reportSchedulerRun(`device-command-sweep`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.commandsDeleted} deleted`,
    })
    if (result.commandsDeleted > 0) {
      console.log(
        `[device-command-sweep] deleted ${result.commandsDeleted} terminal command row(s)`
      )
    }
  } catch (err) {
    reportSchedulerRun(`device-command-sweep`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[device-command-sweep] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process sweep scheduler — call once at boot (server-bun.ts).
// Double-start-guarded within the process.
export function startDeviceCommandSweepScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
