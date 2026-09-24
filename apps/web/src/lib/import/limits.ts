// EXP-630: bounds shared by the adapters and the applier.

// Bytes above this are not imported: the attachment route caps uploads at
// 50 MB, and a migration should not spend the storage budget on one file.
export const IMPORT_MAX_ASSET_BYTES = 50 * 1024 * 1024

// One asset download (connect + body) may take at most this long; the
// applier's retry wraps it, so a stalled file store costs the file, never
// the job.
export const IMPORT_ASSET_FETCH_TIMEOUT_MS = 60 * 1000

// Issues per write transaction — enough to keep Electric shape deltas few,
// small enough that a failed batch re-uploads little.
export const IMPORT_BATCH_SIZE = 25

// Downloaded asset bytes a batch may hold in memory before the applier
// flushes it early (a 25-issue batch of 50 MB screen recordings would
// otherwise pin 1.25 GB).
export const IMPORT_BATCH_ASSET_FLUSH_BYTES = 200 * 1024 * 1024

// `imports.ingest` accepts a raw bundle as one JSON body; these keep a
// hostile or runaway script from parking an unbounded snapshot in
// `import_jobs.payload`. Generous: a Linear workspace with 50k issues is a
// big one.
export const IMPORT_MAX_BUNDLE_ISSUES = 50_000
export const IMPORT_MAX_BUNDLE_COMMENTS = 500_000
