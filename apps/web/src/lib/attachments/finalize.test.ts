import { describe, it } from "vitest"

// EXP-988 contract tests for `finalizeAttachmentUpload` (lib/attachments/
// finalize.ts). Skipped until EXP-955 lands the implementation; that leaf
// un-skips them (and may add cases) but keeps every case here green.
describe.skip(`finalizeAttachmentUpload (EXP-955)`, () => {
  it(`a signed-URL upload of N bytes reports sizeBytes = N`, async () => {
    // Arrange: a reserved row (sizeBytes 0) whose object holds N bytes.
    // Act: finalizeAttachmentUpload(row.id).
    // Assert: the returned row and the stored row both read sizeBytes === N,
    // and contentType is the stored object's.
  })

  it(`refuses a row whose object is not in storage yet`, async () => {
    // A HEAD miss throws; the row stays at 0 so a retry can finalize later.
  })

  it(`is idempotent: finalizing twice writes the same numbers`, async () => {})

  it(`backfills a legacy row with sizeBytes = 0 through the same function`, async () => {})
})
