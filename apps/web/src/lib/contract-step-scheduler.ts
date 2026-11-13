// Deferred schema contract steps (#ERR-1, db/contract-steps.ts): record the
// shipped steps at boot, then apply the due ones every few minutes. Started
// once from server-bun.ts.

import { db } from "@/db/connection"
import { applyDueContractSteps, recordContractSteps } from "@/db/contract-runner"
import { CONTRACT_STEPS } from "@/db/contract-steps"
import { reportSchedulerRun } from "@/lib/metrics/registry"

// One cheap pass a minute: a failed boot record retries, a due step runs
// within a minute of its grace (an empty CONTRACT_STEPS = no query).
const PASS_INTERVAL_MS = 60 * 1000

let started = false
let recorded = false
let running = false

async function pass(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    if (!recorded) {
      await recordContractSteps(db, CONTRACT_STEPS)
      recorded = true
    }
    const applied = await applyDueContractSteps(db, CONTRACT_STEPS)
    reportSchedulerRun(`contract-steps`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${CONTRACT_STEPS.length} shipped, applied ${applied.join(`, `) || `none`}`,
    })
    if (applied.length > 0) {
      console.log(`[contract-steps] applied ${applied.join(`, `)}`)
    }
  } catch (err) {
    reportSchedulerRun(`contract-steps`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[contract-steps] pass failed:`, err)
  } finally {
    running = false
  }
}

export function startContractStepScheduler(): void {
  if (started) return
  started = true
  // Recording at boot is what resets a rolled-back build's pending steps.
  void pass()
  setInterval(() => {
    void pass()
  }, PASS_INTERVAL_MS)
}
