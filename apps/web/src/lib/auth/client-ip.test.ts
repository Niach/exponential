import { describe, expect, it } from "vitest"
import { withProxyAttestedClientIp } from "@/lib/auth/client-ip"

describe(`withProxyAttestedClientIp`, () => {
  it(`keeps only the last (proxy-attested) x-forwarded-for hop`, () => {
    const request = new Request(`https://app.test/api/auth/sign-in/email`, {
      headers: { "x-forwarded-for": `6.6.6.6, 203.0.113.7` },
    })
    expect(
      withProxyAttestedClientIp(request).headers.get(`x-forwarded-for`)
    ).toBe(`203.0.113.7`)
  })

  it(`collapses a long spoofed chain to the appended peer`, () => {
    const request = new Request(`https://app.test/api/auth/sign-in/email`, {
      headers: { "x-forwarded-for": `1.1.1.1,2.2.2.2, 3.3.3.3,203.0.113.7` },
    })
    expect(
      withProxyAttestedClientIp(request).headers.get(`x-forwarded-for`)
    ).toBe(`203.0.113.7`)
  })

  it(`returns the request untouched for a single already-attested hop`, () => {
    const request = new Request(`https://app.test/api/auth/sign-in/email`, {
      headers: { "x-forwarded-for": `203.0.113.7` },
    })
    expect(withProxyAttestedClientIp(request)).toBe(request)
  })

  it(`returns the request untouched without the header (proxy-less fail-open)`, () => {
    const request = new Request(`https://app.test/api/auth/sign-in/email`)
    expect(withProxyAttestedClientIp(request)).toBe(request)
  })

  it(`preserves method, body and other headers on rewrite`, async () => {
    const request = new Request(`https://app.test/api/auth/sign-in/email`, {
      method: `POST`,
      headers: {
        "x-forwarded-for": `6.6.6.6, 203.0.113.7`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({ email: `a@b.c` }),
    })
    const rewritten = withProxyAttestedClientIp(request)
    expect(rewritten.method).toBe(`POST`)
    expect(rewritten.url).toBe(`https://app.test/api/auth/sign-in/email`)
    expect(rewritten.headers.get(`content-type`)).toBe(`application/json`)
    expect(await rewritten.json()).toEqual({ email: `a@b.c` })
  })
})
