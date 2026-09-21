import { describe, expect, it } from "vitest"
import { buildSecurityHeaders } from "./security-headers"

const csp = (env: Parameters<typeof buildSecurityHeaders>[0]) =>
  buildSecurityHeaders(env)[`Content-Security-Policy`].split(`; `)

describe(`buildSecurityHeaders`, () => {
  it(`keeps includeSubDomains on cloud only`, () => {
    expect(
      buildSecurityHeaders({ CLOUD_INSTANCE: `true` })[
        `Strict-Transport-Security`
      ]
    ).toBe(`max-age=63072000; includeSubDomains`)
    for (const CLOUD_INSTANCE of [undefined, ``, `false`, `1`]) {
      expect(
        buildSecurityHeaders({ CLOUD_INSTANCE })[`Strict-Transport-Security`]
      ).toBe(`max-age=63072000`)
    }
  })

  it(`allows any https image so OIDC and Gravatar avatars render`, () => {
    expect(csp({})).toContain(`img-src 'self' data: blob: https:`)
  })

  it(`adds ws: to connect-src only on a plain-http base`, () => {
    expect(csp({ BETTER_AUTH_URL: `http://192.168.1.20` })).toContain(
      `connect-src 'self' https: wss: ws:`
    )
    for (const BETTER_AUTH_URL of [
      `https://issues.example.com`,
      undefined,
      `not a url`,
    ]) {
      expect(csp({ BETTER_AUTH_URL })).toContain(
        `connect-src 'self' https: wss:`
      )
    }
  })

  it(`emits the full header set`, () => {
    expect(Object.keys(buildSecurityHeaders({})).sort()).toEqual([
      `Content-Security-Policy`,
      `Referrer-Policy`,
      `Strict-Transport-Security`,
      `X-Content-Type-Options`,
      `X-Frame-Options`,
    ])
  })
})
