import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// The one outbound door for server-side MCP work. A POST (token exchange,
// refresh, client registration) NEVER follows a redirect, cloud or
// self-host: a 307/308 from the authorization server would re-post the
// refresh token or client secret to wherever it pointed. GET discovery may
// still follow off-cloud (a self-host's LAN provider behind a redirecting
// front). The fake provider (test-provider.ts) sits behind a wrapper that
// plays fetch's redirect semantics for one hop, so the assertion is on what
// the far side RECEIVED, not on the option the code passed.

import { McpHttpError, mcpFetch } from "@/lib/mcp-oauth/net"
import { AS_ISSUER, fakeProvider, type FakeProvider } from "@/lib/mcp-oauth/test-provider"

const BOUNCE = `${AS_ISSUER}/old-token`
const BOUNCE_WELL_KNOWN = `${AS_ISSUER}/.well-known/old-metadata`

/** fetch() as undici behaves on ONE 307 hop: `follow` re-issues the same
 * method + body at the Location, `error` rejects with the wrapped
 * "unexpected redirect", `manual` hands the 3xx back. */
function redirectingFetch(provider: FakeProvider, hops: Record<string, string>) {
  return async (input: RequestInfo | URL, init: RequestInit = {}): Promise<Response> => {
    const url = typeof input === `string` ? input : input.toString()
    const target = hops[url]
    if (!target) return provider.fetch(input, init)
    provider.calls.push({ method: (init.method ?? `GET`).toUpperCase(), url })
    if (init.redirect === `error`) {
      throw new TypeError(`fetch failed`, { cause: new Error(`unexpected redirect`) })
    }
    if (init.redirect === `manual`) {
      return new Response(null, { status: 307, headers: { location: target } })
    }
    return provider.fetch(target, init)
  }
}

const ORIGINAL_CLOUD = process.env.CLOUD_INSTANCE
let provider: FakeProvider
beforeEach(() => {
  // Self-host posture: the permissive side of the guard.
  delete process.env.CLOUD_INSTANCE
  provider = fakeProvider()
  vi.stubGlobal(
    `fetch`,
    redirectingFetch(provider, {
      [BOUNCE]: `${AS_ISSUER}/token`,
      [BOUNCE_WELL_KNOWN]: `${AS_ISSUER}/.well-known/oauth-authorization-server`,
    })
  )
})
afterEach(() => {
  vi.unstubAllGlobals()
  if (ORIGINAL_CLOUD === undefined) delete process.env.CLOUD_INSTANCE
  else process.env.CLOUD_INSTANCE = ORIGINAL_CLOUD
})

describe(`mcpFetch redirects`, () => {
  it(`a 307 on a token POST is an error, never followed (self-host too)`, async () => {
    const body = new URLSearchParams({
      grant_type: `refresh_token`,
      refresh_token: `rt-secret`,
      client_id: `c`,
      client_secret: `cs-secret`,
    }).toString()
    await expect(
      mcpFetch(BOUNCE, {
        method: `POST`,
        headers: { "content-type": `application/x-www-form-urlencoded` },
        body,
      })
    ).rejects.toBeInstanceOf(McpHttpError)
    await expect(
      mcpFetch(BOUNCE, { method: `POST`, body })
    ).rejects.toThrow(`answered with a redirect`)
    // The far side saw the bounce and nothing else: no form reached
    // /token, so the refresh token and client secret went nowhere.
    expect(provider.tokenForms).toHaveLength(0)
    expect(provider.calls.map((call) => call.url)).toEqual([BOUNCE, BOUNCE])
  })

  it(`a DCR registration POST is refused the same way`, async () => {
    vi.stubGlobal(
      `fetch`,
      redirectingFetch(provider, { [`${AS_ISSUER}/old-register`]: `${AS_ISSUER}/register` })
    )
    await expect(
      mcpFetch(`${AS_ISSUER}/old-register`, {
        method: `POST`,
        headers: { "content-type": `application/json` },
        body: JSON.stringify({ client_name: `Exponential` }),
      })
    ).rejects.toBeInstanceOf(McpHttpError)
    expect(provider.registrations).toHaveLength(0)
  })

  it(`GET discovery still follows a redirect off the cloud`, async () => {
    const response = await mcpFetch(BOUNCE_WELL_KNOWN, {
      headers: { accept: `application/json` },
    })
    expect(response.status).toBe(200)
    expect(((await response.json()) as { issuer: string }).issuer).toBe(AS_ISSUER)
    expect(provider.calls.map((call) => call.url)).toEqual([
      BOUNCE_WELL_KNOWN,
      `${AS_ISSUER}/.well-known/oauth-authorization-server`,
    ])
  })

  it(`GET discovery refuses a redirect on the cloud`, async () => {
    process.env.CLOUD_INSTANCE = `true`
    // The cloud guard also resolves the host; a literal public address
    // skips DNS. Redirect handling is what this case is about.
    vi.stubGlobal(
      `fetch`,
      redirectingFetch(provider, { [`https://93.184.216.34/.well-known/x`]: `${AS_ISSUER}/.well-known/oauth-authorization-server` })
    )
    await expect(
      mcpFetch(`https://93.184.216.34/.well-known/x`, { headers: { accept: `application/json` } })
    ).rejects.toThrow(`answered with a redirect`)
    expect(provider.calls.map((call) => call.url)).toEqual([`https://93.184.216.34/.well-known/x`])
  })
})
