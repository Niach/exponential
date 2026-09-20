// EXP-955: heal attachment rows whose `size_bytes` is still 0.
//
// Two kinds of row land there: pre-EXP-955 legacy rows that never recorded a
// size, and signed-URL uploads (EXP-929) whose bytes reached storage but
// whose finalize call never came (the agent died between the curl and the
// second MCP call). Both are fixed by the ONE finalize function — the sweep
// only decides WHICH rows to run it on: `size_bytes = 0` and older than the
// grace window, so a signed upload still mid-flight is never touched (its
// HEAD would miss anyway, but there is no point racing the finalizer).
//
// Runs through the Bun-native object probe: this module sits in the server
// ENTRY graph (server-bun.ts) and must not reach @aws-sdk/client-s3 (see
// storage/bun-s3-cleanup.ts). Backfill mode never refuses or deletes: a HEAD
// miss just leaves the row for the next pass (the mint's own expiry is what
// eventually reaps an upload that never happened).

import { and, eq, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import {
  finalizeAttachmentUploadWith,
  type AttachmentObjectProbe,
} from "@/lib/attachments/finalize-core"
import { bunAttachmentObjectProbe } from "@/lib/storage/bun-s3-cleanup"
import { reportSchedulerRun } from "@/lib/metrics/registry"

/** A signed upload gets this long to finalize itself before the sweep looks. */
export const SIZE_BACKFILL_GRACE_MS = 15 * 60 * 1000

const INITIAL_DELAY_MS = 3 * 60 * 1000
const SWEEP_INTERVAL_MS = 6 * 60 * 60 * 1000

// Bounds one pass; a backlog drains over the following passes. Rows whose
// object is missing stay due, so the cap also bounds how much HEAD traffic a
// pile of abandoned mints can cause per interval.
const MAX_BATCH = 1000

/** Pure due predicate: a 0-size row older than the grace window. */
export function isSizeBackfillDue(
  sizeBytes: number,
  createdAt: Date,
  now: Date = new Date()
): boolean {
  return (
    sizeBytes === 0 &&
    createdAt.getTime() + SIZE_BACKFILL_GRACE_MS <= now.getTime()
  )
}

export interface SizeBackfillResult {
  /** Rows the pass looked at. */
  candidates: number
  /** Rows that now carry a real size. */
  healed: number
  /** Rows whose object is not in storage (left at 0 for a later pass). */
  missing: number
  /** Rows the finalize threw on for any other reason. */
  failed: number
}

/**
 * One backfill pass over the due rows, finalize-in-backfill-mode each.
 * Injectable probe + clock for tests; the scheduler passes the Bun probe.
 */
export async function runAttachmentSizeBackfill(
  probe: AttachmentObjectProbe,
  now: Date = new Date()
): Promise<SizeBackfillResult> {
  const cutoff = new Date(now.getTime() - SIZE_BACKFILL_GRACE_MS)
  const due = await db
    .select({ id: attachments.id })
    .from(attachments)
    .where(
      and(eq(attachments.sizeBytes, 0), lte(attachments.createdAt, cutoff))
    )
    .limit(MAX_BATCH)

  const result: SizeBackfillResult = {
    candidates: due.length,
    healed: 0,
    missing: 0,
    failed: 0,
  }

  for (const { id } of due) {
    try {
      const row = await finalizeAttachmentUploadWith(id, probe, {
        mode: `backfill`,
      })
      if (row.sizeBytes > 0) result.healed += 1
      else result.missing += 1
    } catch (error) {
      const code = (error as { code?: unknown } | null)?.code
      if (code === `PRECONDITION_FAILED` || code === `NOT_FOUND`) {
        result.missing += 1
      } else {
        result.failed += 1
        console.error(
          `[attachment-size-backfill] finalize failed for ${id}:`,
          error
        )
      }
    }
  }

  return result
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    const probe = bunAttachmentObjectProbe()
    if (!probe) {
      reportSchedulerRun(`attachment-size-backfill`, {
        ok: false,
        durationMs: performance.now() - startMs,
        error: `Bun.S3Client unavailable`,
      })
      return
    }
    const result = await runAttachmentSizeBackfill(probe)
    reportSchedulerRun(`attachment-size-backfill`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.candidates} due, ${result.healed} healed, ${result.missing} missing, ${result.failed} failed`,
    })
    if (result.healed > 0 || result.failed > 0) {
      console.log(
        `[attachment-size-backfill] ${result.healed} row(s) healed, ${result.missing} without bytes, ${result.failed} failed`
      )
    }
  } catch (err) {
    reportSchedulerRun(`attachment-size-backfill`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[attachment-size-backfill] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process backfill scheduler — call once at boot
// (server-bun.ts). Double-start-guarded within the process; finalize is
// idempotent, so two instances racing on the same row write the same size.
export function startAttachmentSizeBackfillScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
