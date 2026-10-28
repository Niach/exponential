import { describe, expect, it, vi } from "vitest"
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

  // NEW-85: behind a second appending proxy the client is one hop further in.
  it(`honours TRUSTED_PROXY_HOPS`, () => {
    vi.stubEnv(`TRUSTED_PROXY_HOPS`, `2`)
    try {
      const request = new Request(`https://app.test/api/auth/sign-in/email`, {
        headers: { "x-forwarded-for": `6.6.6.6, 198.51.100.4, 203.0.113.7` },
      })
      expect(
        withProxyAttestedClientIp(request).headers.get(`x-forwarded-for`)
      ).toBe(`198.51.100.4`)
    } finally {
      vi.unstubAllEnvs()
    }
  })

  // FEED-159: Better Auth's buckets group an IPv6 subscriber line by its /64
  // like the widget limiter; IPv4 and IPv4-mapped clients key on the IPv4.
  it.each([
    [`6.6.6.6, 2001:db8:1:2:aaaa:bbbb:cccc:dddd`, `2001:db8:1:2::`],
    [`2001:db8:1:2::9`, `2001:db8:1:2::`],
    [`6.6.6.6, ::ffff:203.0.113.7`, `203.0.113.7`],
    [`6.6.6.6, 203.0.113.7`, `203.0.113.7`],
  ])(`rewrites %s to the limiter address %s`, (header, expected) => {
    const request = new Request(`https://app.test/api/auth/sign-in/email`, {
      headers: { "x-forwarded-for": header },
    })
    expect(withProxyAttestedClientIp(request).headers.get(`x-forwarded-for`)).toBe(expected)
  })

  it(`buckets two interface ids of one /64 together`, () => {
    const at = (ip: string) =>
      withProxyAttestedClientIp(
        new Request(`https://app.test/api/auth/sign-in/email`, { headers: { "x-forwarded-for": ip } })
      ).headers.get(`x-forwarded-for`)
    expect(at(`2001:db8:1:2::1`)).toBe(at(`2001:db8:1:2:ffff::7`))
    expect(at(`2001:db8:1:2::1`)).not.toBe(at(`2001:db8:1:3::1`))
  })
})
