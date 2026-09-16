import { describe, expect, it } from "vitest"
import { pageTitle } from "@/lib/page-title"

describe(`pageTitle`, () => {
  it(`joins the parts most-specific first and ends on the product`, () => {
    expect(pageTitle(`Labels`, `Settings`)).toBe(
      `Labels · Settings · Exponential`
    )
    expect(pageTitle()).toBe(`Exponential`)
  })

  it(`skips parts that have not resolved`, () => {
    expect(pageTitle(undefined, ``, `  `, null, `Inbox`)).toBe(
      `Inbox · Exponential`
    )
  })
})
