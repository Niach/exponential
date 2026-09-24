import { describe, expect, it } from "vitest"
import {
  placeholderNameFromEmail,
  providerProfileFromClaims,
  resolvePlaceholderIdentity,
} from "@/lib/placeholder-members"

// EXP-630 placeholder members — the pure decisions: what an invite names the
// row it creates, and which provider profile replaces that name on claim.

describe(`resolvePlaceholderIdentity`, () => {
  it(`keeps the typed name and normalizes the address`, () => {
    expect(
      resolvePlaceholderIdentity({ email: `  Hannes.Robier@YouSpi.com `, name: ` Hannes Robier ` })
    ).toEqual({ email: `hannes.robier@youspi.com`, name: `Hannes Robier` })
  })

  it(`falls back to the mailbox local part like the sign-in-code default`, () => {
    expect(resolvePlaceholderIdentity({ email: `dennis@straehhuber.com` })).toEqual({
      email: `dennis@straehhuber.com`,
      name: `dennis`,
    })
    expect(resolvePlaceholderIdentity({ email: `dennis@straehhuber.com`, name: `   ` }).name).toBe(
      `dennis`
    )
    expect(placeholderNameFromEmail(`@nolocal`)).toBe(`@nolocal`)
  })
})

describe(`providerProfileFromClaims`, () => {
  it(`reads Google's name and picture`, () => {
    expect(
      providerProfileFromClaims({
        name: `Dennis Strähhuber`,
        picture: `https://lh3.googleusercontent.com/a/x`,
        email: `dennis@example.com`,
      })
    ).toEqual({ name: `Dennis Strähhuber`, image: `https://lh3.googleusercontent.com/a/x` })
  })

  it(`assembles an OIDC given + family name and ignores a non-https picture`, () => {
    expect(
      providerProfileFromClaims({ given_name: `Ada`, family_name: `Lovelace`, picture: `data:x` })
    ).toEqual({ name: `Ada Lovelace` })
  })

  it(`returns nothing for Apple's name-less token or junk`, () => {
    expect(providerProfileFromClaims({ email: `x@privaterelay.appleid.com`, sub: `1` })).toEqual({})
    expect(providerProfileFromClaims(null)).toEqual({})
    expect(providerProfileFromClaims(`str`)).toEqual({})
    expect(providerProfileFromClaims({ name: `   ` })).toEqual({})
  })
})
