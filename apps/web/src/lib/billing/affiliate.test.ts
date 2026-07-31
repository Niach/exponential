import { describe, expect, it } from "vitest"
import { withCreemRef } from "@/lib/billing/affiliate"

const CHECKOUT = `https://checkout.creem.io/checkout/ch_123?foo=bar`

describe(`withCreemRef`, () => {
  it(`appends the token to the checkout URL`, () => {
    const url = new URL(withCreemRef(CHECKOUT, `tok_abc`))
    expect(url.searchParams.get(`creem_ref`)).toBe(`tok_abc`)
    expect(url.searchParams.get(`foo`)).toBe(`bar`)
  })

  it(`never overwrites a creem_ref already on the URL`, () => {
    const withExisting = `${CHECKOUT}&creem_ref=tok_original`
    expect(withCreemRef(withExisting, `tok_other`)).toBe(withExisting)
  })

  it(`is a no-op without a token or on an unparsable URL`, () => {
    expect(withCreemRef(CHECKOUT, null)).toBe(CHECKOUT)
    expect(withCreemRef(CHECKOUT, undefined)).toBe(CHECKOUT)
    expect(withCreemRef(CHECKOUT, ``)).toBe(CHECKOUT)
    expect(withCreemRef(`not a url`, `tok_abc`)).toBe(`not a url`)
  })
})
