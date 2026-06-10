import { describe, expect, it } from "vitest"
import { isOriginAllowed } from "./origin"

describe(`isOriginAllowed`, () => {
  it(`allows any origin when the allowlist is empty`, () => {
    expect(isOriginAllowed(`https://anything.dev`, null, [])).toEqual({
      allowed: true,
      echoOrigin: `https://anything.dev`,
    })
  })

  it(`allows requests without origin when the allowlist is empty`, () => {
    expect(isOriginAllowed(null, null, [])).toEqual({
      allowed: true,
      echoOrigin: null,
    })
  })

  it(`denies requests without origin or referer when restricted`, () => {
    expect(isOriginAllowed(null, null, [`example.com`]).allowed).toBe(false)
  })

  it(`matches exact hostnames case-insensitively`, () => {
    expect(
      isOriginAllowed(`https://Example.COM`, null, [`example.com`]).allowed
    ).toBe(true)
    expect(
      isOriginAllowed(`https://other.com`, null, [`example.com`]).allowed
    ).toBe(false)
  })

  it(`matches any port when the pattern has none`, () => {
    expect(
      isOriginAllowed(`http://localhost:5173`, null, [`localhost`]).allowed
    ).toBe(true)
  })

  it(`matches an explicit port exactly`, () => {
    expect(
      isOriginAllowed(`http://localhost:5173`, null, [`localhost:5173`])
        .allowed
    ).toBe(true)
    expect(
      isOriginAllowed(`http://localhost:4000`, null, [`localhost:5173`])
        .allowed
    ).toBe(false)
  })

  it(`treats default ports as their scheme defaults`, () => {
    expect(
      isOriginAllowed(`https://example.com`, null, [`example.com:443`]).allowed
    ).toBe(true)
    expect(
      isOriginAllowed(`http://example.com`, null, [`example.com:80`]).allowed
    ).toBe(true)
  })

  it(`wildcard matches subdomains but not the apex`, () => {
    const domains = [`*.example.com`]
    expect(
      isOriginAllowed(`https://app.example.com`, null, domains).allowed
    ).toBe(true)
    expect(
      isOriginAllowed(`https://a.b.example.com`, null, domains).allowed
    ).toBe(true)
    expect(isOriginAllowed(`https://example.com`, null, domains).allowed).toBe(
      false
    )
    expect(
      isOriginAllowed(`https://notexample.com`, null, domains).allowed
    ).toBe(false)
  })

  it(`falls back to the referer when origin is missing`, () => {
    const result = isOriginAllowed(null, `https://app.acme.io/checkout?x=1`, [
      `app.acme.io`,
    ])
    expect(result).toEqual({ allowed: true, echoOrigin: `https://app.acme.io` })
  })

  it(`rejects non-http(s) origins on restricted keys`, () => {
    expect(
      isOriginAllowed(`chrome-extension://abc`, null, [`example.com`]).allowed
    ).toBe(false)
  })

  it(`never echoes a denied origin`, () => {
    const result = isOriginAllowed(`https://evil.dev`, null, [`example.com`])
    expect(result).toEqual({ allowed: false, echoOrigin: null })
  })
})
