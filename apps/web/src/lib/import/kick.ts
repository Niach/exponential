// EXP-630: a latch between the router and the worker. The router imports
// this file (never the worker, which imports the routers — that would be a
// cycle); the worker registers its sweep here at boot. `connect`, `ingest`
// and `start` kick it so a single-replica server reacts at once instead of
// waiting for the next 5-second tick.
//
// Under `bun dev` nothing runs server-bun.ts, so no scheduler ever
// registers: the first kick then starts the worker itself through a DYNAMIC
// import (no static cycle), which is also what makes the wizard usable in
// development.
let kicker: (() => void) | null = null
let starting: Promise<void> | null = null

export function registerImportWorkerKick(fn: () => void): void {
  kicker = fn
}

export function kickImportWorker(): void {
  if (kicker) {
    kicker()
    return
  }
  starting ??= import(`@/lib/import/worker`).then((worker) => {
    worker.startImportWorkerScheduler()
    kicker?.()
  })
  starting.catch((err) => {
    console.error(`[import] could not start the import worker:`, err)
    starting = null
  })
}
