import { describe, it } from "vitest"

// EXP-988 contract tests for `runChain` (lib/sessions/run-chain.ts). Skipped
// until EXP-974 lands the implementation.
describe.skip(`runChain (EXP-974)`, () => {
  it(`returns just the session when nothing resumed it and it resumed nothing`, () => {})

  it(`walks resumedFromId backwards to the first row`, () => {
    // c.resumedFromId = b, b.resumedFromId = a; runChain(rows, 'c') → [a, b, c]
  })

  it(`walks forwards to the latest resume from a middle or first row`, () => {
    // runChain(rows, 'a') → [a, b, c]; runChain(rows, 'b') → [a, b, c]
  })

  it(`follows the newest successor at a fork`, () => {})

  it(`yields [] for an unknown id`, () => {})

  it(`tolerates a predecessor the sweep deleted (dangling resumedFromId)`, () => {})
})
