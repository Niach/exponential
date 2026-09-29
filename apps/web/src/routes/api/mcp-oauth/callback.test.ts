import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// EXP-792: the anonymous OAuth callback. State-gated, single-use and TTL
// bound; a good code is exchanged SERVER-side with the flow's encrypted
// verifier and lands the member's encrypted credential; a flow with a
// return path 302s back with `?mcp=connected|failed&server=`; every
// refusal is the same plain "expired" page. The db is the in-memory fake on
// the real schema tables; fetch is a fake provider.

const h = vi.hoisted(() => ({
  db: null as unknown,
  // The Better Auth session the callback's browser carries (null = none).
  sessionUserId: `owner` as string | null,
}))

vi.mock(`@/db/connection`, () => ({
  get db() {
    return h.db
  },
}))
vi.mock(`@/lib/auth`, () => ({
  auth: {
    api: {
      getSession: async () =>
        h.sessionUserId ? { user: { id: h.sessionUserId }, session: {} } : null,
    },
  },
}))
vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: () => `https://app.exponential.dev`,
}))

import {
  handleMcpOauthCallback,
  resetMcpOauthCallbackLimiter,
} from "@/lib/mcp-oauth/callback"
import { beginFlow } from "@/lib/mcp-oauth/flows"
import { credentialAad, decryptCredential } from "@/lib/mcp-oauth/crypto"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"
import { MCP_URL, fakeProvider, type FakeProvider } from "@/lib/mcp-oauth/test-provider"
import type { McpServer } from "@/db/schema"

const TEAM = `44444444-4444-4444-8444-444444444444`
const SERVER = `11111111-1111-4111-8111-111111111111`

const serverRow = {
  id: SERVER,
  teamId: TEAM,
  name: `Linear <Prod>`,
  transport: `http`,
  url: MCP_URL,
  auth: `oauth`,
  headerNames: [],
  envNames: [],
  args: [],
  scopes: [],
}

function request(
  query: Record<string, string | undefined>,
  ip = `203.0.113.7`
): Request {
  const url = new URL(`https://app.exponential.dev/api/mcp-oauth/callback`)
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) url.searchParams.set(key, value)
  }
  return new Request(url, { headers: { "x-forwarded-for": ip } })
}

const ORIGINAL_SECRET = process.env.BETTER_AUTH_SECRET
let db: FakeDb
let provider: FakeProvider

/** A real pending flow via beginFlow; returns its state. */
async function startFlow(returnTo: string | null = null): Promise<string> {
  const { authorizeUrl } = await beginFlow(db, {
    server: serverRow as unknown as McpServer,
    userId: `owner`,
    returnTo,
  })
  return new URL(authorizeUrl).searchParams.get(`state`)!
}

beforeEach(() => {
  process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
  resetMcpOauthCallbackLimiter()
  h.sessionUserId = `owner`
  provider = fakeProvider({ cimd: true })
  vi.stubGlobal(`fetch`, provider.fetch)
  db = createFakeDb({ mcp_servers: [serverRow] })
  h.db = db
})
afterEach(() => {
  vi.unstubAllGlobals()
  process.env.BETTER_AUTH_SECRET = ORIGINAL_SECRET
})

describe(`GET /api/mcp-oauth/callback`, () => {
  it(`exchanges the code server-side and stores the member's credential`, async () => {
    const state = await startFlow()
    const res = await handleMcpOauthCallback(request({ code: `SECRET-CODE`, state }))
    expect(res.status).toBe(200)
    expect(res.headers.get(`content-type`)).toContain(`text/html`)
    expect(res.headers.get(`cache-control`)).toBe(`no-store`)
    const html = await res.text()
    expect(html).toContain(`Connected to Linear &lt;Prod&gt;. You can close this tab.`)
    expect(html).not.toContain(`SECRET-CODE`)
    expect(html).not.toContain(`at-1`)

    expect(provider.tokenForms[0]!.get(`code`)).toBe(`SECRET-CODE`)
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({ status: `done` })
    const [credential] = db.rows(`mcp_credentials`)
    expect(credential).toMatchObject({ serverId: SERVER, userId: `owner`, teamId: TEAM })
    expect(decryptCredential(String(credential!.ciphertext), credentialAad(SERVER, `owner`))).toMatchObject({
      accessToken: `at-1`,
      refreshToken: `rt-1`,
    })
  })

  it(`refuses a browser signed in as someone else, or not at all, and burns the flow`, async () => {
    for (const sessionUserId of [`attacker`, null]) {
      const state = await startFlow(`/t/acme/settings/mcp-servers`)
      h.sessionUserId = sessionUserId
      const res = await handleMcpOauthCallback(request({ code: `c`, state }))
      expect(res.status).toBe(200)
      expect(await res.text()).toContain(`as the member who started the sign-in`)
      const flow = db.rows(`mcp_oauth_flows`).find((row) => row.state === state)!
      expect(flow.status).toBe(`failed`)
      // A later request from the right member finds the flow gone.
      h.sessionUserId = `owner`
      const again = await handleMcpOauthCallback(request({ code: `c`, state }))
      expect(await again.text()).toContain(`This sign-in link has expired.`)
    }
    expect(provider.tokenForms).toHaveLength(0)
    expect(db.rows(`mcp_credentials`)).toHaveLength(0)
  })

  it(`302s to the flow's return path with the outcome`, async () => {
    const state = await startFlow(`/t/acme/settings/mcp-servers`)
    const res = await handleMcpOauthCallback(request({ code: `c`, state }))
    expect(res.status).toBe(302)
    expect(res.headers.get(`location`)).toBe(
      `/t/acme/settings/mcp-servers?mcp=connected&server=${SERVER}`
    )
  })

  it(`a refused exchange fails the flow and redirects with the error`, async () => {
    const state = await startFlow(`/t/acme/settings/mcp-servers`)
    vi.stubGlobal(`fetch`, async () =>
      new Response(JSON.stringify({ error: `invalid_grant` }), { status: 400 })
    )
    const res = await handleMcpOauthCallback(request({ code: `c`, state }))
    expect(res.status).toBe(302)
    const location = new URL(res.headers.get(`location`)!, `https://x.invalid`)
    expect(location.searchParams.get(`mcp`)).toBe(`failed`)
    expect(location.searchParams.get(`server`)).toBe(SERVER)
    expect(location.searchParams.get(`error`)).toMatch(/invalid_grant/)
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({ status: `failed` })
    expect(db.rows(`mcp_credentials`)).toHaveLength(0)
  })

  it(`is single-use: a replayed redirect gets the expired page`, async () => {
    const state = await startFlow()
    await handleMcpOauthCallback(request({ code: `c1`, state }))
    const res = await handleMcpOauthCallback(request({ code: `c2`, state }))
    expect(await res.text()).toContain(`This sign-in link has expired.`)
    expect(provider.tokenForms).toHaveLength(1)
  })

  it(`refuses an unknown, missing or over-long state with the same page`, async () => {
    await startFlow()
    for (const query of [
      { code: `c`, state: `nope` },
      { code: `c` },
      { code: `c`, state: `x`.repeat(65) },
    ]) {
      const res = await handleMcpOauthCallback(request(query))
      expect(res.status).toBe(200)
      expect(await res.text()).toContain(`This sign-in link has expired.`)
    }
    expect(provider.tokenForms).toHaveLength(0)
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`pending`)
  })

  it(`refuses a flow past the 10-minute TTL`, async () => {
    const state = await startFlow()
    db.rows(`mcp_oauth_flows`)[0]!.createdAt = new Date(Date.now() - 10 * 60_000)
    const res = await handleMcpOauthCallback(request({ code: `c`, state }))
    expect(await res.text()).toContain(`This sign-in link has expired.`)
    expect(provider.tokenForms).toHaveLength(0)
  })

  it(`a provider error fails the flow and says why`, async () => {
    const state = await startFlow()
    const res = await handleMcpOauthCallback(
      request({ state, error: `access_denied`, error_description: `User <cancelled>` })
    )
    expect(res.status).toBe(200)
    expect(await res.text()).toContain(
      `Connecting Linear &lt;Prod&gt; failed: access_denied: User &lt;cancelled&gt;`
    )
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      status: `failed`,
      error: `access_denied: User <cancelled>`,
    })
    expect(provider.tokenForms).toHaveLength(0)
  })

  it(`a redirect without a code fails the flow too`, async () => {
    const state = await startFlow()
    const res = await handleMcpOauthCallback(request({ state }))
    expect(await res.text()).toContain(`failed: no code`)
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({ status: `failed`, error: `no code` })
  })

  it(`rate limits per IP (burst 20) with a 429 and no db work`, async () => {
    const state = await startFlow()
    for (let i = 0; i < 20; i += 1) {
      const res = await handleMcpOauthCallback(request({ state: `nope` }))
      expect(res.status).toBe(200)
    }
    const limited = await handleMcpOauthCallback(request({ code: `c`, state }))
    expect(limited.status).toBe(429)
    expect(limited.headers.get(`retry-after`)).toMatch(/^\d+$/)
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`pending`)
    // Another address is unaffected.
    const other = await handleMcpOauthCallback(request({ code: `c`, state }, `198.51.100.9`))
    expect(other.status).toBe(200)
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`done`)
  })
})
