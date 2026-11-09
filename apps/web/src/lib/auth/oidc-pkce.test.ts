import { describe, expect, it } from "vitest"
import { createS256Cache, supportsS256 } from "./oidc-pkce"

describe(`supportsS256`, () => {
  it(`is true only when code_challenge_methods_supported lists S256`, () => {
    expect(supportsS256({ code_challenge_methods_supported: [`plain`, `S256`] })).toBe(true)
    expect(supportsS256({ code_challenge_methods_supported: [`plain`] })).toBe(false)
    expect(supportsS256({ code_challenge_methods_supported: `S256` })).toBe(false)
    expect(supportsS256({ issuer: `https://idp.test` })).toBe(false)
    expect(supportsS256(null)).toBe(false)
    expect(supportsS256(`S256`)).toBe(false)
  })
})

describe(`createS256Cache`, () => {
  const p = { id: `idp`, discoveryUrl: `https://idp.test/.well-known/openid-configuration` }

  it(`caches a reachable answer and re-asks after the TTL`, async () => {
    let t = 0
    let calls = 0
    const cache = createS256Cache(async () => {
      calls++
      return { code_challenge_methods_supported: [`S256`] }
    }, () => t)
    expect(await cache.resolve(p)).toBe(true)
    expect(await cache.resolve(p)).toBe(true)
    expect(calls).toBe(1)
    t += 60 * 60 * 1000 + 1
    expect(await cache.resolve(p)).toBe(true)
    expect(calls).toBe(2)
  })

  it(`treats an unreachable IdP as no PKCE and asks again next time`, async () => {
    let up = false
    const cache = createS256Cache(async () => {
      if (!up) throw new Error(`ECONNREFUSED`)
      return { code_challenge_methods_supported: [`S256`] }
    })
    expect(await cache.resolve(p)).toBe(false)
    up = true
    expect(await cache.resolve(p)).toBe(true)
  })

  it(`shares one fetch between concurrent sign-ins`, async () => {
    let calls = 0
    const cache = createS256Cache(async () => {
      calls++
      await new Promise((r) => setTimeout(r, 5))
      return {}
    })
    expect(await Promise.all([cache.resolve(p), cache.resolve(p)])).toEqual([false, false])
    expect(calls).toBe(1)
  })
})
