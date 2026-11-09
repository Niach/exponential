import { describe, expect, it } from "vitest"
import { ownOriginTargets, shouldTraceRequest } from "./client-sdk"

describe(`trace propagation targets`, () => {
  const targets = ownOriginTargets(`https://app.exponential.at`)
  const propagates = (url: string) => targets.some((t) => t.test(url))

  it(`covers our own origin only`, () => {
    expect(propagates(`/api/trpc/issues.list`)).toBe(true)
    expect(propagates(`https://app.exponential.at/api/shapes/issues`)).toBe(true)
    expect(propagates(`https://app.exponential.at`)).toBe(true)
    expect(propagates(`//evil.example/x`)).toBe(false)
    expect(propagates(`https://app.exponential.at.evil.example/x`)).toBe(false)
    expect(propagates(`https://api.github.com/x`)).toBe(false)
  })
})

describe(`shouldTraceRequest`, () => {
  it(`skips only live shape long-polls`, () => {
    expect(shouldTraceRequest(`/api/shapes/issues?offset=-1`)).toBe(true)
    expect(shouldTraceRequest(`/api/shapes/issues?offset=1_0&live=true&handle=h`)).toBe(false)
    expect(shouldTraceRequest(`/api/trpc/x?live=true`)).toBe(true)
  })
})
