import { describe, expect, it } from "vitest"
import { NotImplementedError } from "./not-implemented"

// EXP-988: the contract's handler stubs. Each leaf REPLACES the matching
// assertion here with its own tests when it fills the file; until then the
// stub has to say so loudly rather than return a plausible empty result.
describe(`MCP handler stubs (EXP-988)`, () => {
  // EXP-979 filled attachments-list.ts: see attachments-list.test.ts.
  // EXP-929 filled attachments-upload.ts: see attachments-upload.test.ts.
  // EXP-936 filled `sessions-compact.ts`: its tests live beside it
  // (`sessions-compact.test.ts`).

  it(`the stub marker names what is missing and who owns it`, () => {
    const error = new NotImplementedError(`a_tool`, `EXP-0`)
    expect(error.name).toBe(`NotImplementedError`)
    expect(error.message).toBe(`a_tool is not implemented yet (EXP-0)`)
  })
})
