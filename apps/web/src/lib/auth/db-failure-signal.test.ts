import { describe, expect, it } from "vitest"

import {
  authDbFailureCount,
  withAuthDbFailureSignal,
} from "@/lib/auth/db-failure-signal"

// REV2-20: the counter is what lets resolveSession tell a swallowed lookup
// failure apart from a genuine "no session" — see db-failure-signal.ts.

function makeAdapter(overrides: Record<string, unknown> = {}) {
  return {
    id: `test-adapter`,
    findOne: async (arg: string) => `found:${arg}`,
    ...overrides,
  }
}

describe(`withAuthDbFailureSignal`, () => {
  it(`passes results and non-function members through untouched`, async () => {
    const adapter = withAuthDbFailureSignal(() => makeAdapter())()
    const before = authDbFailureCount()
    expect(adapter.id).toBe(`test-adapter`)
    await expect(adapter.findOne(`session`)).resolves.toBe(`found:session`)
    expect(authDbFailureCount()).toBe(before)
  })

  it(`counts a rejected adapter call and rethrows the original error`, async () => {
    const boom = new Error(`connection terminated unexpectedly`)
    const adapter = withAuthDbFailureSignal(() =>
      makeAdapter({
        findOne: async () => {
          throw boom
        },
      })
    )()
    const before = authDbFailureCount()
    await expect(adapter.findOne(`session`)).rejects.toBe(boom)
    expect(authDbFailureCount()).toBe(before + 1)
  })

  it(`counts a synchronous throw too`, () => {
    const boom = new Error(`pool destroyed`)
    const adapter = withAuthDbFailureSignal(() =>
      makeAdapter({
        findOne: () => {
          throw boom
        },
      })
    )()
    const before = authDbFailureCount()
    expect(() => adapter.findOne(`session`)).toThrow(boom)
    expect(authDbFailureCount()).toBe(before + 1)
  })

  it(`returns a stable function reference per method`, () => {
    const adapter = withAuthDbFailureSignal(() => makeAdapter())()
    expect(adapter.findOne).toBe(adapter.findOne)
  })
})
