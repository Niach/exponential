import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// EXP-792: the server-run MCP OAuth sign-in — the URL rules (return paths,
// the CIMD gate), discovery (PRM path-suffixed FIRST, then AS metadata),
// the client choice (CIMD on a public https base, else a cached DCR
// client), the authorize URL (PKCE S256 + resource + scope) and the code
// exchange that lands the member's encrypted credential. The db is the
// in-memory fake on the real schema tables; fetch is a fake provider.

vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: () => `https://app.exponential.dev`,
}))

import { createHash } from "node:crypto"
import {
  beginFlow,
  completeFlow,
  flowAcceptsCode,
  MCP_OAUTH_FLOW_TTL_MS,
} from "@/lib/mcp-oauth/flows"
import {
  isPublicHttpsBase,
  returnUrl,
  safeReturnTo,
} from "@/lib/mcp-oauth/urls"
import { discover, scopeFor, wellKnownCandidates } from "@/lib/mcp-oauth/oauth-client"
import {
  credentialAad,
  decryptCredential,
  decryptSecret,
  flowVerifierAad,
} from "@/lib/mcp-oauth/crypto"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"
import { AS_ISSUER, MCP_URL, fakeProvider } from "@/lib/mcp-oauth/test-provider"
import type { McpOauthFlow, McpServer } from "@/db/schema"

const TEAM = `44444444-4444-4444-8444-444444444444`
const SERVER = `11111111-1111-4111-8111-111111111111`

const server = (over: Partial<McpServer> = {}): McpServer => ({
  id: SERVER,
  teamId: TEAM,
  name: `Linear`,
  transport: `http`,
  url: MCP_URL,
  headerNames: [],
  command: null,
  args: [],
  envNames: [],
  scopes: [],
  auth: `oauth`,
  enabledByDefault: true,
  createdById: `owner`,
  createdAt: new Date(0),
  updatedAt: new Date(0),
  ...over,
})

const ORIGINAL_SECRET = process.env.BETTER_AUTH_SECRET
let db: FakeDb
beforeEach(() => {
  process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
  db = createFakeDb({ mcp_servers: [server()] })
})
afterEach(() => {
  vi.unstubAllGlobals()
  process.env.BETTER_AUTH_SECRET = ORIGINAL_SECRET
})

describe(`return paths`, () => {
  it(`accepts same-origin relative paths only`, () => {
    expect(safeReturnTo(`/t/acme/settings/mcp-servers`)).toBe(`/t/acme/settings/mcp-servers`)
    expect(safeReturnTo(`/t/acme/settings/mcp-servers?tab=x#y`)).toBe(
      `/t/acme/settings/mcp-servers?tab=x#y`
    )
    for (const bad of [
      `//evil.example/x`,
      `/\\evil.example`,
      `https://evil.example/`,
      `javascript:alert(1)`,
      `relative/path`,
      `/ok\nLocation: x`,
      // Dot segments that RESOLVE to a protocol-relative `//evil.com`.
      `/.//evil.com`,
      `/a/..//evil.com`,
      `/%2e//evil.com`,
      `/%2E%2E//evil.com`,
      ``,
      null,
      undefined,
    ]) {
      expect(safeReturnTo(bad), String(bad)).toBeNull()
    }
  })

  it(`returns the normalized path`, () => {
    expect(safeReturnTo(`/t/acme/./settings/../settings/mcp-servers`)).toBe(
      `/t/acme/settings/mcp-servers`
    )
  })

  it(`never emits a protocol-relative path`, () => {
    for (const hostile of [`//evil.com/x`, `/.//evil.com`, `/a/..//evil.com`, `/%2e//evil.com`]) {
      const out = returnUrl(hostile, { mcp: `connected` })
      expect(out.startsWith(`//`), `${hostile} -> ${out}`).toBe(false)
      expect(out.startsWith(`/`)).toBe(true)
    }
  })

  it(`appends the outcome params, keeping the path's own query and hash`, () => {
    expect(
      returnUrl(`/t/acme/settings/mcp-servers?tab=x#y`, {
        mcp: `failed`,
        server: SERVER,
        error: `a b&c`,
      })
    ).toBe(`/t/acme/settings/mcp-servers?tab=x&mcp=failed&server=${SERVER}&error=a+b%26c#y`)
  })
})

describe(`isPublicHttpsBase (the CIMD gate)`, () => {
  it(`is true only for https on a routable, dotted host`, () => {
    expect(isPublicHttpsBase(`https://app.exponential.dev`)).toBe(true)
    for (const base of [
      `http://app.exponential.dev`,
      `https://localhost:3000`,
      `https://192.168.1.5`,
      `https://10.0.0.2`,
      `https://nas`,
      `https://exp.local`,
      `https://box.internal`,
      `not a url`,
    ]) {
      expect(isPublicHttpsBase(base), base).toBe(false)
    }
  })
})

describe(`flowAcceptsCode`, () => {
  it(`is pending-only and TTL-bound`, () => {
    const now = new Date()
    expect(flowAcceptsCode({ status: `pending`, createdAt: now }, now)).toBe(true)
    expect(flowAcceptsCode({ status: `done`, createdAt: now }, now)).toBe(false)
    expect(
      flowAcceptsCode(
        { status: `pending`, createdAt: new Date(now.getTime() - MCP_OAUTH_FLOW_TTL_MS) },
        now
      )
    ).toBe(false)
  })
})

describe(`discovery`, () => {
  it(`tries the path-suffixed well-known FIRST, then the root`, () => {
    expect(wellKnownCandidates(`https://mcp.sentry.dev/mcp/`, `oauth-protected-resource`)).toEqual([
      `https://mcp.sentry.dev/.well-known/oauth-protected-resource/mcp`,
      `https://mcp.sentry.dev/.well-known/oauth-protected-resource`,
    ])
    expect(wellKnownCandidates(`https://auth.example.com`, `openid-configuration`)).toEqual([
      `https://auth.example.com/.well-known/openid-configuration`,
    ])
  })

  it(`follows the PRM to the AS metadata`, async () => {
    const provider = fakeProvider({ cimd: true })
    vi.stubGlobal(`fetch`, provider.fetch)
    const discovery = await discover(MCP_URL)
    expect(discovery).toMatchObject({
      issuer: AS_ISSUER,
      authorizationEndpoint: `${AS_ISSUER}/authorize`,
      tokenEndpoint: `${AS_ISSUER}/token`,
      cimdSupported: true,
      scopesSupported: [`read`, `write`],
      prmFound: true,
    })
    expect(provider.calls[0]!.url).toBe(
      `https://mcp.example.com/.well-known/oauth-protected-resource/mcp`
    )
  })

  it(`refuses AS metadata naming another issuer (RFC 8414 §3.3)`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider({ metadata: { issuer: `https://evil.example.com` } }).fetch)
    await expect(discover(MCP_URL)).rejects.toThrow(/names issuer https:\/\/evil\.example\.com/)
    vi.stubGlobal(`fetch`, fakeProvider({ metadata: { issuer: undefined } }).fetch)
    await expect(discover(MCP_URL)).rejects.toThrow(/names issuer none/)
    // A trailing slash is the same issuer.
    vi.stubGlobal(`fetch`, fakeProvider({ metadata: { issuer: `${AS_ISSUER}/` } }).fetch)
    await expect(discover(MCP_URL)).resolves.toMatchObject({ issuer: AS_ISSUER })
  })

  it(`refuses non-https endpoints for an https server`, async () => {
    for (const [field, value] of [
      [`authorization_endpoint`, `javascript:alert(1)`],
      [`authorization_endpoint`, `http://auth.example.com/authorize`],
      [`token_endpoint`, `http://auth.example.com/token`],
      [`registration_endpoint`, `data:text/html,x`],
    ] as const) {
      vi.stubGlobal(`fetch`, fakeProvider({ metadata: { [field]: value } }).fetch)
      await expect(discover(MCP_URL), `${field}=${value}`).rejects.toThrow(/must be an https/)
    }
  })

  it(`allows http endpoints only for a loopback http server`, async () => {
    const local = `http://localhost:8787/mcp`
    vi.stubGlobal(`fetch`, async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url === `http://localhost:8787/.well-known/oauth-authorization-server`) {
        return new Response(
          JSON.stringify({
            issuer: `http://localhost:8787`,
            authorization_endpoint: `http://localhost:8787/authorize`,
            token_endpoint: `http://localhost:8787/token`,
          }),
          { headers: { "content-type": `application/json` } }
        )
      }
      return new Response(`not found`, { status: 404 })
    })
    await expect(discover(local)).resolves.toMatchObject({
      authorizationEndpoint: `http://localhost:8787/authorize`,
    })
  })

  it(`scope = the row's scopes, else the advertised ones, else none`, () => {
    expect(scopeFor([`issues:read`], { scopesSupported: [`read`] })).toBe(`issues:read`)
    expect(scopeFor([], { scopesSupported: [`read`, `write`] })).toBe(`read write`)
    expect(scopeFor([], { scopesSupported: [] })).toBeNull()
  })
})

describe(`beginFlow`, () => {
  it(`uses the CIMD client id on a public base and builds a PKCE S256 authorize URL`, async () => {
    const provider = fakeProvider({ cimd: true })
    vi.stubGlobal(`fetch`, provider.fetch)
    const flow = await beginFlow(db, {
      server: server(),
      userId: `actor`,
      returnTo: `/t/acme/settings/mcp-servers`,
    })
    const url = new URL(flow.authorizeUrl)
    expect(`${url.origin}${url.pathname}`).toBe(`${AS_ISSUER}/authorize`)
    const q = url.searchParams
    expect(q.get(`client_id`)).toBe(`https://app.exponential.dev/api/mcp-oauth/client.json`)
    expect(q.get(`redirect_uri`)).toBe(`https://app.exponential.dev/api/mcp-oauth/callback`)
    expect(q.get(`code_challenge_method`)).toBe(`S256`)
    expect(q.get(`resource`)).toBe(MCP_URL)
    expect(q.get(`scope`)).toBe(`read write`)
    expect(provider.registrations).toHaveLength(0)

    const [row] = db.rows(`mcp_oauth_flows`) as unknown as McpOauthFlow[]
    expect(row).toMatchObject({
      state: q.get(`state`),
      userId: `actor`,
      teamId: TEAM,
      serverId: SERVER,
      issuer: AS_ISSUER,
      tokenEndpoint: `${AS_ISSUER}/token`,
      resource: MCP_URL,
      returnTo: `/t/acme/settings/mcp-servers`,
      status: `pending`,
    })
    // The verifier is stored ENCRYPTED, and it is the challenge's preimage.
    const verifier = decryptSecret(row!.codeVerifierCiphertext, flowVerifierAad(String(row!.state)))!
    expect(row!.codeVerifierCiphertext).not.toContain(verifier)
    expect(createHash(`sha256`).update(verifier).digest(`base64url`)).toBe(
      q.get(`code_challenge`)
    )
  })

  it(`registers a DCR client on a non-public base, once per (issuer, redirect)`, async () => {
    const provider = fakeProvider({ cimd: true })
    vi.stubGlobal(`fetch`, provider.fetch)
    const args = { server: server(), userId: `actor`, returnTo: null, appBase: `http://localhost:3000` }
    const first = await beginFlow(db, args)
    const second = await beginFlow(db, args)
    expect(provider.registrations).toEqual([
      expect.objectContaining({
        redirect_uris: [`http://localhost:3000/api/mcp-oauth/callback`],
        token_endpoint_auth_method: `none`,
      }),
    ])
    expect(new URL(first.authorizeUrl).searchParams.get(`client_id`)).toBe(`dcr-client-1`)
    expect(new URL(second.authorizeUrl).searchParams.get(`client_id`)).toBe(`dcr-client-1`)
    expect(db.rows(`mcp_oauth_clients`)).toHaveLength(1)
  })

  it(`keys the DCR cache on the registration endpoint too`, async () => {
    const args = { server: server(), userId: `actor`, returnTo: null, appBase: `http://localhost:3000` }
    vi.stubGlobal(`fetch`, fakeProvider().fetch)
    await beginFlow(db, args)
    const other = fakeProvider({ metadata: { registration_endpoint: `${AS_ISSUER}/register-2` } })
    vi.stubGlobal(`fetch`, other.fetch)
    const second = await beginFlow(db, args)
    // Another registration endpoint never reuses (or overwrites) the first client.
    expect(other.registrations).toHaveLength(1)
    expect(db.rows(`mcp_oauth_clients`).map((row) => row.registrationEndpoint)).toEqual([
      `${AS_ISSUER}/register`,
      `${AS_ISSUER}/register-2`,
    ])
    expect(new URL(second.authorizeUrl).searchParams.get(`client_id`)).toBe(`dcr-client-1`)
  })

  it(`refuses a provider with neither CIMD nor registration on a private base`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider({ registration: false }).fetch)
    await expect(
      beginFlow(db, { server: server(), userId: `actor`, returnTo: null, appBase: `http://localhost:3000` })
    ).rejects.toThrow(/pre-registered OAuth client/)
  })
})

describe(`completeFlow`, () => {
  it(`exchanges the code with the verifier and stores the member's encrypted tokens`, async () => {
    const provider = fakeProvider({ cimd: true })
    vi.stubGlobal(`fetch`, provider.fetch)
    await beginFlow(db, { server: server(), userId: `actor`, returnTo: null })
    const flow = db.rows(`mcp_oauth_flows`)[0] as unknown as McpOauthFlow
    const now = new Date(`2026-09-29T12:00:00Z`)
    await completeFlow(db, flow, `CODE-1`, now)

    const form = provider.tokenForms[0]!
    expect(form.get(`grant_type`)).toBe(`authorization_code`)
    expect(form.get(`code`)).toBe(`CODE-1`)
    expect(form.get(`code_verifier`)).toBe(decryptSecret(flow.codeVerifierCiphertext, flowVerifierAad(flow.state)))
    expect(form.get(`client_id`)).toBe(flow.clientId)
    expect(form.get(`resource`)).toBe(MCP_URL)

    const [credential] = db.rows(`mcp_credentials`)
    expect(credential).toMatchObject({
      serverId: SERVER,
      userId: `actor`,
      teamId: TEAM,
      issuer: AS_ISSUER,
      clientId: flow.clientId,
      error: null,
      expiresAt: new Date(`2026-09-29T13:00:00Z`),
    })
    expect(String(credential!.ciphertext)).not.toContain(`at-1`)
    expect(decryptCredential(String(credential!.ciphertext), credentialAad(SERVER, `actor`))).toEqual({
      accessToken: `at-1`,
      refreshToken: `rt-1`,
      tokenType: `bearer`,
      tokenEndpoint: `${AS_ISSUER}/token`,
    })
  })
})
