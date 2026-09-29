import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// EXP-792: per-member MCP credentials — the connection status a member
// sees per server, and the launch resolution: oauth → a Bearer header
// (refreshed server-side when expiring, the rotated refresh token stored),
// secret → the one declared header/env, anything unusable SKIPPED with a
// reason (never a launch blocker).

vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: () => `https://app.exponential.dev`,
}))

import {
  connectionFor,
  resolveForLaunch,
  storeOauthTokens,
  storeSecret,
} from "@/lib/mcp-oauth/credentials"
import { credentialAad, decryptCredential, encryptCredential } from "@/lib/mcp-oauth/crypto"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"
import { AS_ISSUER, MCP_URL, fakeProvider } from "@/lib/mcp-oauth/test-provider"

const TEAM = `44444444-4444-4444-8444-444444444444`
const OAUTH = `11111111-1111-4111-8111-111111111111`
const SECRET_HTTP = `11111111-1111-4111-8111-222222222222`
const SECRET_STDIO = `11111111-1111-4111-8111-333333333333`
const OPEN = `11111111-1111-4111-8111-444444444444`
const NOW = new Date(`2026-09-29T12:00:00Z`)

const ORIGINAL_SECRET = process.env.BETTER_AUTH_SECRET
let db: FakeDb
beforeEach(() => {
  process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
  db = createFakeDb({
    mcp_servers: [
      { id: OAUTH, teamId: TEAM, name: `linear`, transport: `http`, url: MCP_URL, auth: `oauth`, headerNames: [], envNames: [], args: [], scopes: [] },
      { id: SECRET_HTTP, teamId: TEAM, name: `grafana`, transport: `http`, url: `https://g.example.com/mcp`, auth: `secret`, headerNames: [`X-Api-Key`], envNames: [], args: [], scopes: [] },
      { id: SECRET_STDIO, teamId: TEAM, name: `pg`, transport: `stdio`, url: null, command: `npx`, args: [`pg-mcp`], auth: `secret`, headerNames: [], envNames: [`PG_URL`], scopes: [] },
      { id: OPEN, teamId: TEAM, name: `docs`, transport: `http`, url: `https://docs.example.com/mcp`, auth: `none`, headerNames: [], envNames: [], args: [], scopes: [] },
    ],
  })
})
afterEach(() => {
  vi.unstubAllGlobals()
  process.env.BETTER_AUTH_SECRET = ORIGINAL_SECRET
})

async function seedOauth(over: { expiresAt?: Date | null; refreshToken?: string | null } = {}) {
  await storeOauthTokens(
    db,
    {
      serverId: OAUTH,
      userId: `actor`,
      teamId: TEAM,
      issuer: AS_ISSUER,
      clientId: `https://app.exponential.dev/api/mcp-oauth/client.json`,
      tokenEndpoint: `${AS_ISSUER}/token`,
      tokens: {
        accessToken: `at-0`,
        refreshToken: over.refreshToken === undefined ? `rt-0` : over.refreshToken,
        tokenType: `Bearer`,
        expiresAt: over.expiresAt === undefined ? new Date(NOW.getTime() + 3600_000) : over.expiresAt,
      },
    },
    NOW
  )
}

describe(`connectionFor`, () => {
  const oauth = { auth: `oauth` }
  const row = (payload: object, over: Record<string, unknown> = {}) => ({
    serverId: OAUTH,
    userId: `actor`,
    ciphertext: encryptCredential(payload, credentialAad(OAUTH, `actor`)),
    expiresAt: null as Date | null,
    error: null as string | null,
    ...over,
  })

  it(`none-auth servers need nothing`, () => {
    expect(connectionFor({ auth: `none` }, null).status).toBe(`not_needed`)
  })

  it(`maps oauth rows to connected / expired / error / not_connected`, () => {
    const past = new Date(NOW.getTime() - 1000)
    expect(connectionFor(oauth, null, NOW).status).toBe(`not_connected`)
    expect(connectionFor(oauth, row({ accessToken: `a`, refreshToken: `r` }, { expiresAt: past }), NOW)).toEqual({
      status: `connected`,
      expiresAt: past.toISOString(),
      error: null,
    })
    expect(connectionFor(oauth, row({ accessToken: `a` }, { expiresAt: past }), NOW).status).toBe(`expired`)
    expect(connectionFor(oauth, row({ accessToken: `a`, refreshToken: `r` }, { error: `invalid_grant` }), NOW)).toEqual({
      status: `error`,
      expiresAt: null,
      error: `invalid_grant`,
    })
  })

  it(`an undecryptable row (rotated secret) reads as not connected`, () => {
    const sealed = row({ accessToken: `a` })
    process.env.BETTER_AUTH_SECRET = `test-secret-bbbbbbbbbbbbbbbbbbbbbbbbbbbb`
    expect(connectionFor(oauth, sealed, NOW).status).toBe(`not_connected`)
    expect(connectionFor({ auth: `secret` }, sealed, NOW).status).toBe(`not_connected`)
  })

  it(`secret servers: connected once a value is stored`, () => {
    expect(connectionFor({ auth: `secret` }, row({ value: `k` })).status).toBe(`connected`)
    expect(connectionFor({ auth: `secret` }, row({ accessToken: `a` })).status).toBe(`not_connected`)
  })
})

describe(`resolveForLaunch`, () => {
  it(`hands the caller's own values out, skipping what is not connected`, async () => {
    await seedOauth()
    await storeSecret(db, { serverId: SECRET_HTTP, userId: `actor`, teamId: TEAM, value: `key-1` }, NOW)
    // A teammate's credential never leaks into the caller's launch.
    await storeSecret(db, { serverId: SECRET_STDIO, userId: `mate`, teamId: TEAM, value: `postgres://mate` }, NOW)
    const result = await resolveForLaunch(
      db,
      { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH, SECRET_HTTP, SECRET_STDIO, OPEN, `99999999-9999-4999-8999-999999999999`] },
      NOW
    )
    expect(result.servers).toEqual([
      expect.objectContaining({ id: OAUTH, transport: `http`, url: MCP_URL, headers: [{ name: `Authorization`, value: `Bearer at-0` }], env: [] }),
      expect.objectContaining({ id: SECRET_HTTP, headers: [{ name: `X-Api-Key`, value: `key-1` }] }),
      expect.objectContaining({ id: OPEN, headers: [], env: [] }),
    ])
    expect(result.skipped).toEqual([
      { id: SECRET_STDIO, name: `pg`, reason: `not connected` },
      expect.objectContaining({ id: `99999999-9999-4999-8999-999999999999` }),
    ])
    expect(result.warnings).toEqual([])
  })

  it(`a stdio secret rides as the declared env var`, async () => {
    await storeSecret(db, { serverId: SECRET_STDIO, userId: `actor`, teamId: TEAM, value: `postgres://me` }, NOW)
    const result = await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [SECRET_STDIO] }, NOW)
    expect(result.servers[0]).toMatchObject({
      transport: `stdio`,
      command: `npx`,
      args: [`pg-mcp`],
      headers: [],
      env: [{ name: `PG_URL`, value: `postgres://me` }],
    })
  })

  it(`servers of teams the caller is not in are skipped`, async () => {
    await seedOauth()
    const result = await resolveForLaunch(db, { userId: `actor`, teamIds: [], serverIds: [OAUTH] }, NOW)
    expect(result.servers).toEqual([])
    expect(result.skipped).toHaveLength(1)
  })

  it(`refreshes a token expiring within 10 minutes and persists the rotation`, async () => {
    const provider = fakeProvider()
    vi.stubGlobal(`fetch`, provider.fetch)
    await seedOauth({ expiresAt: new Date(NOW.getTime() + 5 * 60_000) })
    const result = await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] }, NOW)
    expect(result.servers[0]!.headers).toEqual([{ name: `Authorization`, value: `Bearer at-1` }])
    const form = provider.tokenForms[0]!
    expect(form.get(`grant_type`)).toBe(`refresh_token`)
    expect(form.get(`refresh_token`)).toBe(`rt-0`)
    expect(form.get(`resource`)).toBe(MCP_URL)
    const [row] = db.rows(`mcp_credentials`)
    expect(decryptCredential(String(row!.ciphertext), credentialAad(OAUTH, `actor`))).toMatchObject({ accessToken: `at-1`, refreshToken: `rt-1` })
    expect(row!.expiresAt).toEqual(new Date(NOW.getTime() + 3600_000))
    expect(row!.error).toBeNull()
  })

  it(`refreshes under a per-row advisory lock`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider().fetch)
    await seedOauth({ expiresAt: new Date(NOW.getTime() + 5 * 60_000) })
    await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] }, NOW)
    expect(db.executed).toHaveLength(1)
    expect(JSON.stringify(db.executed[0])).toContain(`pg_advisory_xact_lock(hashtext(`)
  })

  it(`skips the refresh when another launch refreshed while it waited for the lock`, async () => {
    const provider = fakeProvider()
    vi.stubGlobal(`fetch`, provider.fetch)
    // "Another process" holds the lock, refreshes and commits first.
    db = createFakeDb(
      { mcp_servers: db.rows(`mcp_servers`) },
      {
        onExecute: () => {
          const [row] = db.rows(`mcp_credentials`)
          row!.ciphertext = encryptCredential(
            { accessToken: `at-other`, refreshToken: `rt-other`, tokenType: `Bearer` },
            credentialAad(OAUTH, `actor`)
          )
          row!.expiresAt = new Date(NOW.getTime() + 3600_000)
        },
      }
    )
    await seedOauth({ expiresAt: new Date(NOW.getTime() + 5 * 60_000) })
    const result = await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] }, NOW)
    expect(result.servers[0]!.headers).toEqual([{ name: `Authorization`, value: `Bearer at-other` }])
    expect(provider.tokenForms).toHaveLength(0)
  })

  it(`never overwrites a credential that changed during the refresh (compare-and-swap)`, async () => {
    const provider = fakeProvider()
    vi.stubGlobal(`fetch`, async (input: RequestInfo | URL, init?: RequestInit) => {
      // A reconnect lands while the refresh request is in flight.
      const [row] = db.rows(`mcp_credentials`)
      row!.ciphertext = encryptCredential({ accessToken: `at-reconnect` }, credentialAad(OAUTH, `actor`))
      return provider.fetch(input, init)
    })
    await seedOauth({ expiresAt: new Date(NOW.getTime() + 5 * 60_000) })
    const result = await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] }, NOW)
    // This launch still gets the token it minted…
    expect(result.servers[0]!.headers).toEqual([{ name: `Authorization`, value: `Bearer at-1` }])
    // …but the stored credential stays the reconnect's.
    const [row] = db.rows(`mcp_credentials`)
    expect(decryptCredential(String(row!.ciphertext), credentialAad(OAUTH, `actor`))).toEqual({
      accessToken: `at-reconnect`,
    })
  })

  it(`keeps the old refresh token when the provider rotates none`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider({ refreshRotates: false }).fetch)
    await seedOauth({ expiresAt: new Date(NOW.getTime() - 1000) })
    await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] }, NOW)
    const [row] = db.rows(`mcp_credentials`)
    expect(decryptCredential(String(row!.ciphertext), credentialAad(OAUTH, `actor`))).toMatchObject({ accessToken: `at-1`, refreshToken: `rt-0` })
  })

  it(`a failed refresh stores the error and skips the server`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider({ refreshFails: true }).fetch)
    await seedOauth({ expiresAt: new Date(NOW.getTime() - 1000) })
    const result = await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] }, NOW)
    expect(result.servers).toEqual([])
    expect(result.skipped[0]!.reason).toMatch(/refresh failed: .*refresh token revoked/)
    const [row] = db.rows(`mcp_credentials`)
    expect(row!.error).toMatch(/refresh token revoked/)
    expect(connectionFor({ auth: `oauth` }, row as never, NOW).status).toBe(`error`)
  })

  it(`no refresh token: warns while still valid, skips once expired`, async () => {
    await seedOauth({ expiresAt: new Date(NOW.getTime() + 4 * 60_000), refreshToken: null })
    const soon = await resolveForLaunch(db, { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] }, NOW)
    expect(soon.servers).toHaveLength(1)
    expect(soon.warnings).toEqual([`linear: access token expires in 4 min and cannot be refreshed`])
    const later = await resolveForLaunch(
      db,
      { userId: `actor`, teamIds: [TEAM], serverIds: [OAUTH] },
      new Date(NOW.getTime() + 5 * 60_000)
    )
    expect(later.servers).toEqual([])
    expect(later.skipped[0]!.reason).toMatch(/expired/)
  })
})
