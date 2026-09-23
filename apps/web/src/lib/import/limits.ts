// EXP-630: bounds shared by the adapters and the applier.

// Bytes above this are not imported: the attachment route caps uploads at
// 50 MB, and a migration should not spend the storage budget on one file.
export const IMPORT_MAX_ASSET_BYTES = 50 * 1024 * 1024

// Issues per write transaction — enough to keep Electric shape deltas few,
// small enough that a failed batch re-uploads little.
export const IMPORT_BATCH_SIZE = 25
