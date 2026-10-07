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
  delegateEntryName,
  delegateHandle,
  memberRoster,
  resolveForLaunch,
  resolveSharedForLaunch,
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
      shared: false,
    })
    expect(connectionFor(oauth, row({ accessToken: `a` }, { expiresAt: past }), NOW).status).toBe(`expired`)
    expect(connectionFor(oauth, row({ accessToken: `a`, refreshToken: `r` }, { error: `invalid_grant` }), NOW)).toEqual({
      status: `error`,
      expiresAt: null,
      error: `invalid_grant`,
      shared: false,
    })
  })

  it(`carries the member's shared flag (FEED-73)`, () => {
    expect(connectionFor(oauth, row({ accessToken: `a` }, { shared: true }), NOW).shared).toBe(true)
    expect(connectionFor(oauth, null, NOW).shared).toBe(false)
    expect(connectionFor({ auth: `none` }, row({ accessToken: `a` }, { shared: true })).shared).toBe(false)
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
      expect.objectContaining({ id: OPEN, headers: [], env: [], actor: null }),
    ])
    // FEED-73: credentialed entries name whose credential they spend.
    expect(result.servers[0]!.actor).toEqual({ userId: `actor`, name: ``, shared: false })
    expect(result.members).toEqual([])
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

describe(`delegateHandle (FEED-73)`, () => {
  it(`slugs the name's first word, lowercased, diacritics folded`, () => {
    expect(delegateHandle(`Chris Doe`, `c@x.io`, new Set())).toBe(`chris`)
    expect(delegateHandle(`  Jörg  Müller`, null, new Set())).toBe(`jorg`)
    expect(delegateHandle(`O'Brien`, null, new Set())).toBe(`o-brien`)
    expect(delegateHandle(`__Ann__`, null, new Set())).toBe(`ann`)
  })

  it(`falls back to the email local part, then to member`, () => {
    expect(delegateHandle(``, `Dana.Smith@example.com`, new Set())).toBe(`dana-smith`)
    expect(delegateHandle(`李`, `li@example.com`, new Set())).toBe(`li`)
    expect(delegateHandle(null, null, new Set())).toBe(`member`)
  })

  it(`cuts to 12 chars and uniques with -2, -3 within the cap`, () => {
    expect(delegateHandle(`Bartholomewxyz`, null, new Set())).toBe(`bartholomewx`)
    const taken = new Set<string>()
    expect(delegateHandle(`Chris A`, null, taken)).toBe(`chris`)
    expect(delegateHandle(`Chris B`, null, taken)).toBe(`chris-2`)
    expect(delegateHandle(`Chris C`, null, taken)).toBe(`chris-3`)
    const long = new Set<string>()
    expect(delegateHandle(`Bartholomewxyz`, null, long)).toBe(`bartholomewx`)
    expect(delegateHandle(`Bartholomewxyz`, null, long)).toBe(`bartholome-2`)
  })

  it(`caps the whole entry name at 30 chars by trimming the handle`, () => {
    expect(delegateEntryName(`linear`, `Chris`, null, new Set())).toBe(`linear-as-chris`)
    const name = delegateEntryName(`a-very-long-server-name`, `Christopher`, null, new Set())
    expect(name).toBe(`a-very-long-server-name-as-chr`)
    expect(name.length).toBe(30)
  })
})

describe(`resolveSharedForLaunch + memberRoster (FEED-73)`, () => {
  const seedShares = () => {
    const cred = (serverId: string, userId: string, payload: object, over: Record<string, unknown> = {}) => ({
      id: `c-${serverId}-${userId}`,
      serverId,
      userId,
      teamId: TEAM,
      ciphertext: encryptCredential(payload, credentialAad(serverId, userId)),
      expiresAt: null,
      issuer: null,
      clientId: null,
      error: null,
      shared: true,
      ...over,
    })
    db = createFakeDb({
      mcp_servers: db.rows(`mcp_servers`),
      team_members: [`actor`, `chris`, `chris2`, `dana`, `eve`].map((userId) => ({ teamId: TEAM, userId })),
      users: [
        { id: `actor`, name: `Actor Person`, email: `actor@x.io` },
        { id: `chris`, name: `Chris Doe`, email: `chris@x.io` },
        { id: `chris2`, name: `Chris Roe`, email: `chris2@x.io` },
        { id: `dana`, name: `Dana`, email: `dana@x.io` },
        { id: `eve`, name: `Eve`, email: `eve@x.io` },
        { id: `gone`, name: `Gone`, email: `gone@x.io` },
      ],
      mcp_credentials: [
        // The caller's own share never doubles as an extra.
        cred(OAUTH, `actor`, { accessToken: `own` }),
        cred(OAUTH, `chris`, { accessToken: `chris-tok` }),
        cred(OAUTH, `chris2`, { accessToken: `chris2-tok` }),
        // Shared but expired with no refresh: unavailable, never skipped.
        cred(OAUTH, `dana`, { accessToken: `dana-tok` }, { expiresAt: new Date(NOW.getTime() - 1000) }),
        // Connected, not shared.
        cred(OAUTH, `eve`, { accessToken: `eve-tok` }, { shared: false }),
        // A departed member's share never resolves.
        cred(OAUTH, `gone`, { accessToken: `gone-tok` }),
        cred(SECRET_HTTP, `chris`, { value: `chris-key` }),
        // stdio shares never resolve for another member.
        cred(SECRET_STDIO, `chris`, { value: `postgres://chris` }),
      ],
    })
  }

  it(`resolves other members' shares as <server>-as-<member> entries with actors`, async () => {
    seedShares()
    const shares = await resolveSharedForLaunch(db, {
      teamId: TEAM,
      serverIds: [OAUTH, SECRET_HTTP, SECRET_STDIO, OPEN],
      excludeUserId: `actor`,
      now: NOW,
    })
    expect(shares.servers.map((entry) => [entry.id, entry.name, entry.actor])).toEqual([
      [OAUTH, `linear-as-chris`, { userId: `chris`, name: `Chris Doe`, shared: true }],
      [OAUTH, `linear-as-chris-2`, { userId: `chris2`, name: `Chris Roe`, shared: true }],
      [SECRET_HTTP, `grafana-as-chris`, { userId: `chris`, name: `Chris Doe`, shared: true }],
    ])
    expect(shares.servers[0]!.headers).toEqual([{ name: `Authorization`, value: `Bearer chris-tok` }])
    expect(shares.servers[2]!.headers).toEqual([{ name: `X-Api-Key`, value: `chris-key` }])
    expect(JSON.stringify(shares.servers)).not.toMatch(/own|eve-tok|gone-tok|postgres/)
    expect([...(shares.unavailable.get(OAUTH) ?? [])]).toEqual([`dana`])

    const roster = await memberRoster(db, {
      teamId: TEAM,
      serverIds: [OAUTH, SECRET_STDIO, OPEN],
      selfUserId: `actor`,
      sharedResolved: shares.resolved,
      unavailable: shares.unavailable,
      now: NOW,
    })
    const states = roster.map((member) => `${member.serverName}:${member.name}:${member.state}`)
    expect(states).toEqual([
      `linear:Actor Person:self`,
      `linear:Chris Doe:shared`,
      `linear:Chris Roe:shared`,
      `linear:Dana:unavailable`,
      `linear:Eve:connected`,
      `pg:Actor Person:self`,
      `pg:Chris Doe:connected`,
      `pg:Chris Roe:not_connected`,
      `pg:Dana:not_connected`,
      `pg:Eve:not_connected`,
    ])
  })
})
