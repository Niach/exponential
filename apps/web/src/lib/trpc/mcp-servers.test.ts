import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-792: the mcpServers router. Owner-only registry writes with the
// cross-field validation matrix; member reads with the caller's connection
// per server; the server-held credential procedures (connect, setSecret,
// disconnect, test, resolveForLaunch) and the owner's probe. The router runs
// against ctx.db, so the in-memory fake (lib/mcp-oauth/test-db.ts) on the
// real schema tables is enough; membership is stubbed and fetch is a fake
// provider (lib/mcp-oauth/test-provider.ts).

const h = vi.hoisted(() => ({
  assertTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
  assertTeamOwner: vi.fn(async () => ({ role: `owner` }) as unknown),
  getUserTeamIds: vi.fn(async () => [`team-1`]),
  appBaseUrl: vi.fn(() => `https://app.exponential.dev`),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  assertTeamOwner: h.assertTeamOwner,
  getUserTeamIds: h.getUserTeamIds,
}))
vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: h.appBaseUrl,
}))

import { mcpServersRouter, retargetsCredentials } from "@/lib/trpc/mcp-servers"
import { isReservedMcpEnvName } from "@exp/db-schema/domain"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"
import { credentialAad, encryptCredential, decryptCredential } from "@/lib/mcp-oauth/crypto"
import { AS_ISSUER, MCP_URL, fakeProvider } from "@/lib/mcp-oauth/test-provider"

const TEAM = `44444444-4444-4444-8444-444444444444`
const SERVER = `11111111-1111-4111-8111-111111111111`
const SERVER_B = `11111111-1111-4111-8111-222222222222`

function serverRow(over: Record<string, unknown> = {}) {
  return {
    id: SERVER,
    teamId: TEAM,
    name: `Linear`,
    transport: `http`,
    url: `https://mcp.linear.app/mcp`,
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
  }
}

const ORIGINAL_SECRET = process.env.BETTER_AUTH_SECRET
let db: FakeDb
// EXP-1140: `sessionId` rides as the launcher's `X-Exp-Session-Id`; `keyId`
// models an api-key request (the plugin's mock session id IS the key row id).
function callerFor(
  userId = `actor`,
  { sessionId, keyId }: { sessionId?: string; keyId?: string } = {}
) {
  return mcpServersRouter.createCaller({
    session: {
      user: { id: userId, name: `Actor`, email: `a@example.com` },
      session: keyId ? { id: keyId } : undefined,
    },
    db,
    request: new Request(`http://localhost/`, {
      headers: sessionId ? { "X-Exp-Session-Id": sessionId } : {},
    }),
  } as never)
}

async function rejectionOf(promise: Promise<unknown>): Promise<TRPCError> {
  return promise.then(
    () => {
      throw new Error(`expected a rejection`)
    },
    (e: unknown) => e as TRPCError
  )
}

beforeEach(() => {
  h.assertTeamMember.mockClear()
  h.assertTeamOwner.mockClear()
  h.assertTeamOwner.mockResolvedValue({ role: `owner` })
  h.getUserTeamIds.mockClear()
  h.getUserTeamIds.mockResolvedValue([TEAM])
  h.appBaseUrl.mockReturnValue(`https://app.exponential.dev`)
  process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
  db = createFakeDb()
})

afterEach(() => {
  vi.unstubAllGlobals()
  process.env.BETTER_AUTH_SECRET = ORIGINAL_SECRET
})

describe(`mcpServers.create — validation matrix`, () => {
  const base = { teamId: TEAM, name: `Linear`, auth: `none` as const }

  it(`creates an http server and returns the row`, async () => {
    const row = await callerFor().create({
      ...base,
      transport: `http`,
      url: `https://mcp.linear.app/mcp`,
      headerNames: [`X-Api-Key`, `X-Api-Key`],
      scopes: [`read`],
      enabledByDefault: true,
    })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM)
    expect(row).toMatchObject({
      teamId: TEAM,
      name: `Linear`,
      transport: `http`,
      url: `https://mcp.linear.app/mcp`,
      headerNames: [`X-Api-Key`],
      command: null,
      args: [],
      envNames: [],
      scopes: [`read`],
      auth: `none`,
      enabledByDefault: true,
      createdById: `actor`,
    })
    expect(db.rows(`mcp_servers`)).toHaveLength(1)
  })

  it(`is owner-only`, async () => {
    h.assertTeamOwner.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    const error = await rejectionOf(
      callerFor().create({ ...base, transport: `http`, url: `https://x.y/mcp` })
    )
    expect(error.code).toBe(`FORBIDDEN`)
    expect(db.rows(`mcp_servers`)).toHaveLength(0)
  })

  it(`refuses a duplicate name in the team`, async () => {
    db = createFakeDb({ mcp_servers: [serverRow()] })
    const error = await rejectionOf(
      callerFor().create({ ...base, transport: `http`, url: `https://x.y/mcp` })
    )
    expect(error.code).toBe(`CONFLICT`)
  })

  it(`http requires an https url (localhost http allowed)`, async () => {
    await expect(
      callerFor().create({ ...base, transport: `http` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({ ...base, transport: `http`, url: `http://x.y/mcp` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({ ...base, transport: `http`, url: `ftp://x.y/mcp` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({
        ...base,
        name: `Local`,
        transport: `http`,
        url: `http://localhost:8080/mcp`,
      })
    ).resolves.toMatchObject({ url: `http://localhost:8080/mcp` })
    await expect(
      callerFor().create({
        ...base,
        name: `Local2`,
        transport: `http`,
        url: `http://127.0.0.1:8080/mcp`,
      })
    ).resolves.toMatchObject({ url: `http://127.0.0.1:8080/mcp` })
  })

  it(`stdio requires a command and drops http-only fields`, async () => {
    await expect(
      callerFor().create({ ...base, transport: `stdio` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const row = await callerFor().create({
      ...base,
      transport: `stdio`,
      command: `npx`,
      args: [`-y`, `@modelcontextprotocol/server-github`],
      envNames: [`GITHUB_TOKEN`],
      headerNames: [`Ignored`],
      url: `https://ignored.example`,
    })
    expect(row).toMatchObject({
      transport: `stdio`,
      command: `npx`,
      args: [`-y`, `@modelcontextprotocol/server-github`],
      envNames: [`GITHUB_TOKEN`],
      headerNames: [],
      url: null,
    })
  })

  it(`validates names, the name/scope caps and the name length`, async () => {
    await expect(
      callerFor().create({
        ...base,
        transport: `http`,
        url: `https://x.y/mcp`,
        headerNames: [`bad header`],
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({
        ...base,
        transport: `http`,
        url: `https://x.y/mcp`,
        headerNames: Array.from({ length: 17 }, (_, i) => `H${i}`),
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({
        ...base,
        transport: `http`,
        url: `https://x.y/mcp`,
        scopes: Array.from({ length: 17 }, (_, i) => `s${i}`),
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({
        ...base,
        name: `x`.repeat(65),
        transport: `http`,
        url: `https://x.y/mcp`,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`auth: secret needs exactly ONE declared name (the secret position)`, async () => {
    const http = { ...base, auth: `secret` as const, transport: `http` as const, url: `https://x.y/mcp` }
    await expect(callerFor().create(http)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(
      callerFor().create({ ...http, headerNames: [`A`, `B`] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({ ...http, headerNames: [`X-Api-Key`] })
    ).resolves.toMatchObject({ auth: `secret`, headerNames: [`X-Api-Key`] })

    const stdio = { ...base, name: `GH`, auth: `secret` as const, transport: `stdio` as const, command: `gh-mcp` }
    await expect(callerFor().create(stdio)).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(
      callerFor().create({ ...stdio, envNames: [`GITHUB_TOKEN`] })
    ).resolves.toMatchObject({ auth: `secret`, envNames: [`GITHUB_TOKEN`] })
  })

  it(`refuses reserved env names (the launcher's own environment)`, async () => {
    const stdio = { ...base, name: `PG`, transport: `stdio` as const, command: `pg-mcp` }
    for (const name of [`PATH`, `path`, `NODE_OPTIONS`, `CLAUDE_CONFIG_DIR`, `ANTHROPIC_BASE_URL`, `EXP_MCP_TOKEN_1`, `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`, `git_dir`]) {
      await expect(
        callerFor().create({ ...stdio, envNames: [name] }),
        name
      ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    }
    await expect(
      callerFor().create({ ...stdio, envNames: [`PG_URL`] })
    ).resolves.toMatchObject({ envNames: [`PG_URL`] })
  })

  it(`auth: oauth is http-only`, async () => {
    await expect(
      callerFor().create({
        ...base,
        auth: `oauth`,
        transport: `stdio`,
        command: `x`,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().create({
        ...base,
        auth: `oauth`,
        transport: `http`,
        url: `https://x.y/mcp`,
      })
    ).resolves.toMatchObject({ auth: `oauth` })
  })
})

describe(`mcpServers.update / remove`, () => {
  beforeEach(() => {
    db = createFakeDb({
      mcp_servers: [
        serverRow(),
        serverRow({ id: SERVER_B, name: `GitHub`, auth: `none` }),
      ],
    })
  })

  it(`validates the MERGED row — flipping auth to secret needs the one name`, async () => {
    await expect(
      callerFor().update({ id: SERVER, auth: `secret` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const row = await callerFor().update({
      id: SERVER,
      auth: `secret`,
      headerNames: [`X-Api-Key`],
    })
    expect(row).toMatchObject({
      id: SERVER,
      auth: `secret`,
      headerNames: [`X-Api-Key`],
      url: `https://mcp.linear.app/mcp`,
    })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM)
  })

  it(`refuses renaming onto another server's name`, async () => {
    const error = await rejectionOf(
      callerFor().update({ id: SERVER, name: `GitHub` })
    )
    expect(error.code).toBe(`CONFLICT`)
    await expect(
      callerFor().update({ id: SERVER, name: `Linear` })
    ).resolves.toMatchObject({ name: `Linear` })
  })

  it(`404s an unknown id and refuses non-owners`, async () => {
    await expect(
      callerFor().update({ id: `33333333-3333-4333-8333-333333333333`, name: `x` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    h.assertTeamOwner.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(callerFor().remove({ id: SERVER })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    expect(db.rows(`mcp_servers`)).toHaveLength(2)
  })

  it(`a re-targeting update drops members' credentials and pending sign-ins`, async () => {
    const seedCredentials = () => {
      db.rows(`mcp_credentials`).splice(0)
      db.rows(`mcp_oauth_flows`).splice(0)
      db.rows(`mcp_credentials`).push(
        { id: `c-1`, serverId: SERVER, userId: `actor`, teamId: TEAM, ciphertext: `x`, expiresAt: null, error: null },
        { id: `c-2`, serverId: SERVER, userId: `mate`, teamId: TEAM, ciphertext: `x`, expiresAt: null, error: null },
        { id: `c-3`, serverId: SERVER_B, userId: `actor`, teamId: TEAM, ciphertext: `x`, expiresAt: null, error: null }
      )
      db.rows(`mcp_oauth_flows`).push(
        { id: `f-1`, serverId: SERVER, userId: `actor`, status: `pending` },
        { id: `f-2`, serverId: SERVER_B, userId: `actor`, status: `pending` }
      )
    }
    // Cosmetic edits keep them.
    seedCredentials()
    await callerFor().update({ id: SERVER, name: `Linear Prod`, enabledByDefault: false, scopes: [`read`] })
    expect(db.rows(`mcp_credentials`)).toHaveLength(3)
    expect(db.rows(`mcp_oauth_flows`)).toHaveLength(2)

    for (const patch of [
      { url: `https://evil.example.com/mcp` },
      { auth: `secret` as const, headerNames: [`X-Api-Key`] },
    ]) {
      seedCredentials()
      await callerFor().update({ id: SERVER, ...patch })
      expect(db.rows(`mcp_credentials`).map((row) => row.id), JSON.stringify(patch)).toEqual([`c-3`])
      expect(db.rows(`mcp_oauth_flows`).map((row) => row.id)).toEqual([`f-2`])
    }
  })

  it(`retargetsCredentials: url, transport, auth, header and env names`, () => {
    const row = {
      url: `https://a.example/mcp`,
      transport: `http`,
      auth: `secret`,
      headerNames: [`X-Key`],
      envNames: [] as string[],
    } as const
    const base = { ...row, headerNames: [...row.headerNames], envNames: [...row.envNames] }
    expect(retargetsCredentials(base, { ...base })).toBe(false)
    expect(retargetsCredentials(base, { ...base, url: `https://b.example/mcp` })).toBe(true)
    expect(retargetsCredentials(base, { ...base, transport: `stdio` })).toBe(true)
    expect(retargetsCredentials(base, { ...base, auth: `oauth` })).toBe(true)
    expect(retargetsCredentials(base, { ...base, headerNames: [`Authorization`] })).toBe(true)
    expect(retargetsCredentials(base, { ...base, envNames: [`TOKEN`] })).toBe(true)
  })

  it(`remove deletes the row`, async () => {
    await expect(callerFor().remove({ id: SERVER })).resolves.toEqual({
      ok: true,
    })
    expect(db.rows(`mcp_servers`).map((r) => r.id)).toEqual([SERVER_B])
  })
})


describe(`mcpServers.list — connection statuses`, () => {
  const SECRET = `11111111-1111-4111-8111-333333333333`
  const OPEN = `11111111-1111-4111-8111-444444444444`
  const LATER = new Date(Date.now() + 3600_000)

  beforeEach(() => {
    db = createFakeDb({
      mcp_servers: [
        serverRow(),
        serverRow({ id: SERVER_B, name: `Sentry` }),
        serverRow({ id: SECRET, name: `Grafana`, auth: `secret`, headerNames: [`X-Api-Key`] }),
        serverRow({ id: OPEN, name: `Docs`, auth: `none` }),
        serverRow({ id: `11111111-1111-4111-8111-555555555555`, teamId: `other-team`, name: `Foreign` }),
      ],
      team_members: [
        { teamId: TEAM, userId: `actor` },
        { teamId: TEAM, userId: `mate` },
        { teamId: TEAM, userId: `third` },
      ],
      mcp_credentials: [
        { serverId: SERVER, userId: `actor`, teamId: TEAM, ciphertext: encryptCredential({ accessToken: `a`, refreshToken: `r` }, credentialAad(SERVER, `actor`)), expiresAt: LATER, error: null },
        { serverId: SERVER, userId: `mate`, teamId: TEAM, ciphertext: encryptCredential({ accessToken: `b` }, credentialAad(SERVER, `mate`)), expiresAt: LATER, error: null, shared: true },
        // A departed member's credential never counts.
        { serverId: SERVER, userId: `gone`, teamId: TEAM, ciphertext: encryptCredential({ accessToken: `c` }, credentialAad(SERVER, `gone`)), expiresAt: LATER, error: null, shared: true },
        // Shared but broken: never counted as shared.
        { serverId: SERVER_B, userId: `actor`, teamId: TEAM, ciphertext: encryptCredential({ accessToken: `a`, refreshToken: `r` }, credentialAad(SERVER_B, `actor`)), expiresAt: null, error: `invalid_grant`, shared: true },
        { serverId: SECRET, userId: `mate`, teamId: TEAM, ciphertext: encryptCredential({ value: `k` }, credentialAad(SECRET, `mate`)), expiresAt: null, error: null },
      ],
    })
  })

  it(`returns the caller's connection per server plus team counts, never a credential`, async () => {
    const rows = await callerFor().list({ teamId: TEAM })
    expect(rows.map((row) => row.name)).toEqual([`Linear`, `Sentry`, `Grafana`, `Docs`])
    const byName = new Map(rows.map((row) => [row.name, row]))
    expect(byName.get(`Linear`)).toMatchObject({
      connection: { status: `connected`, expiresAt: LATER.toISOString(), error: null, shared: false },
      connectedCount: 2,
      memberCount: 3,
      // FEED-73: who shared (connected current members only) and who connected.
      sharedCount: 1,
      sharedUserIds: [`mate`],
      connectedUserIds: [`actor`, `mate`],
    })
    expect(byName.get(`Sentry`)).toMatchObject({
      connection: { status: `error`, error: `invalid_grant`, shared: true },
      connectedCount: 0,
      sharedCount: 0,
      sharedUserIds: [],
      connectedUserIds: [],
    })
    expect(byName.get(`Grafana`)).toMatchObject({
      connection: { status: `not_connected`, shared: false },
      connectedCount: 1,
      connectedUserIds: [`mate`],
    })
    expect(byName.get(`Docs`)).toMatchObject({
      connection: { status: `not_needed` },
      connectedCount: 3,
      sharedCount: 0,
      connectedUserIds: [`actor`, `mate`, `third`],
    })
    expect(JSON.stringify(rows)).not.toContain(`ciphertext`)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
  })
})

describe(`mcpServers.connect`, () => {
  beforeEach(() => {
    db = createFakeDb({ mcp_servers: [serverRow({ url: MCP_URL })] })
  })

  it(`returns the authorize URL and mints a pending flow for the caller`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider({ cimd: true }).fetch)
    const { authorizeUrl } = await callerFor().connect({
      serverId: SERVER,
      returnTo: `/t/acme/settings/mcp-servers`,
    })
    expect(authorizeUrl.startsWith(`${AS_ISSUER}/authorize?`)).toBe(true)
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      userId: `actor`,
      serverId: SERVER,
      teamId: TEAM,
      returnTo: `/t/acme/settings/mcp-servers`,
      status: `pending`,
    })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
  })

  it(`refuses a foreign returnTo, a non-oauth server and non-members`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider({ cimd: true }).fetch)
    for (const returnTo of [`https://evil.example/`, `//evil.example`, `settings`]) {
      await expect(
        callerFor().connect({ serverId: SERVER, returnTo })
      ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    }
    db.rows(`mcp_servers`)[0]!.auth = `none`
    await expect(callerFor().connect({ serverId: SERVER })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
    })
    db.rows(`mcp_servers`)[0]!.auth = `oauth`
    h.assertTeamMember.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(callerFor().connect({ serverId: SERVER })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    expect(db.rows(`mcp_oauth_flows`)).toHaveLength(0)
  })

  it(`a discovery failure is a PRECONDITION_FAILED with the reason`, async () => {
    vi.stubGlobal(`fetch`, async () => new Response(`nope`, { status: 404 }))
    const error = await rejectionOf(callerFor().connect({ serverId: SERVER }))
    expect(error.code).toBe(`PRECONDITION_FAILED`)
    expect(error.message).toMatch(/no OAuth authorization server/)
  })
})

describe(`mcpServers.setSecret / disconnect`, () => {
  const SECRET = `11111111-1111-4111-8111-333333333333`
  beforeEach(() => {
    db = createFakeDb({
      mcp_servers: [
        serverRow(),
        serverRow({ id: SECRET, name: `Grafana`, auth: `secret`, headerNames: [`X-Api-Key`] }),
      ],
    })
  })

  it(`stores the caller's secret encrypted, replaces it, and disconnect forgets it`, async () => {
    await callerFor().setSecret({ serverId: SECRET, value: `key-1` })
    await callerFor().setSecret({ serverId: SECRET, value: `key-2` })
    await callerFor(`mate`).setSecret({ serverId: SECRET, value: `mate-key` })
    const rows = db.rows(`mcp_credentials`)
    expect(rows).toHaveLength(2)
    const mine = rows.find((row) => row.userId === `actor`)!
    expect(String(mine.ciphertext)).not.toContain(`key-2`)
    expect(decryptCredential(String(mine.ciphertext), credentialAad(SECRET, `actor`))).toEqual({ value: `key-2` })

    await expect(callerFor().disconnect({ serverId: SECRET })).resolves.toEqual({ ok: true })
    expect(db.rows(`mcp_credentials`).map((row) => row.userId)).toEqual([`mate`])
  })

  it(`refuses a secret for a non-secret server and a multi-line header value`, async () => {
    await expect(
      callerFor().setSecret({ serverId: SERVER, value: `x` })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    await expect(
      callerFor().setSecret({ serverId: SECRET, value: `a\nb` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      callerFor().setSecret({ serverId: SECRET, value: `` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})

describe(`mcpServers.test / probe`, () => {
  beforeEach(() => {
    db = createFakeDb({ mcp_servers: [serverRow({ url: MCP_URL })] })
  })

  it(`runs initialize + tools/list with the caller's token (SSE answer, session id carried)`, async () => {
    const provider = fakeProvider({ tools: 4 })
    provider.liveTokens.add(`live`)
    vi.stubGlobal(`fetch`, provider.fetch)
    db.rows(`mcp_credentials`).push({
      id: `c-1`,
      serverId: SERVER,
      userId: `actor`,
      teamId: TEAM,
      ciphertext: encryptCredential({ accessToken: `live`, tokenType: `Bearer` }, credentialAad(SERVER, `actor`)),
      expiresAt: null,
      issuer: AS_ISSUER,
      clientId: `x`,
      error: null,
    })
    await expect(callerFor().test({ serverId: SERVER })).resolves.toEqual({
      ok: true,
      tools: 4,
      error: null,
    })
    const methods = provider.mcpRequests.map((request) => request.body.method)
    expect(methods).toEqual([`initialize`, `notifications/initialized`, `tools/list`])
    expect(provider.mcpRequests[0]!.headers.get(`accept`)).toBe(
      `application/json, text/event-stream`
    )
    expect(provider.mcpRequests[2]!.headers.get(`mcp-session-id`)).toBe(`sess-1`)
    expect(provider.mcpRequests[2]!.headers.get(`authorization`)).toBe(`Bearer live`)
  })

  it(`reports not connected, and a refused credential, without throwing`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider().fetch)
    await expect(callerFor().test({ serverId: SERVER })).resolves.toEqual({
      ok: false,
      tools: null,
      error: `not connected`,
    })
    db.rows(`mcp_credentials`).push({
      id: `c-1`,
      serverId: SERVER,
      userId: `actor`,
      teamId: TEAM,
      ciphertext: encryptCredential({ accessToken: `revoked` }, credentialAad(SERVER, `actor`)),
      expiresAt: null,
      error: null,
    })
    const result = await callerFor().test({ serverId: SERVER })
    expect(result.ok).toBe(false)
    expect(result.error).toMatch(/refused the credential \(HTTP 401\)/)
  })

  it(`probe detects an OAuth server and its scopes (owner-only)`, async () => {
    vi.stubGlobal(`fetch`, fakeProvider().fetch)
    await expect(callerFor().probe({ teamId: TEAM, url: MCP_URL })).resolves.toEqual({
      url: MCP_URL,
      suggestedName: `example`,
      auth: `oauth`,
      reachable: true,
      error: null,
      scopes: [`read`, `write`],
    })
    h.assertTeamOwner.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(
      callerFor().probe({ teamId: TEAM, url: MCP_URL })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`probe: an open server is none, an unreachable one says so`, async () => {
    vi.stubGlobal(`fetch`, async () =>
      new Response(JSON.stringify({ jsonrpc: `2.0`, id: 1, result: {} }), {
        status: 200,
        headers: { "content-type": `application/json` },
      })
    )
    await expect(
      callerFor().probe({ teamId: TEAM, url: `https://mcp.linear.app/mcp` })
    ).resolves.toMatchObject({ auth: `none`, reachable: true, suggestedName: `linear` })
    vi.stubGlobal(`fetch`, async () => {
      throw new TypeError(`fetch failed`)
    })
    await expect(
      callerFor().probe({ teamId: TEAM, url: MCP_URL })
    ).resolves.toMatchObject({ reachable: false, error: `could not connect` })
  })

  it(`probe on the cloud refuses a private host`, async () => {
    const previous = process.env.CLOUD_INSTANCE
    process.env.CLOUD_INSTANCE = `true`
    const fetchSpy = vi.fn()
    vi.stubGlobal(`fetch`, fetchSpy)
    try {
      await expect(
        callerFor().probe({ teamId: TEAM, url: `http://localhost:8080/mcp` })
      ).resolves.toMatchObject({ reachable: false, error: expect.stringMatching(/https/) })
      await expect(
        callerFor().probe({ teamId: TEAM, url: `https://127.0.0.1/mcp` })
      ).resolves.toMatchObject({ reachable: false, error: expect.stringMatching(/not a public address/) })
      expect(fetchSpy).not.toHaveBeenCalled()
    } finally {
      process.env.CLOUD_INSTANCE = previous
    }
  })
})

// EXP-1140: the one procedure that hands out decrypted secrets is bound to a
// LIVE run the caller owns or hosts (`X-Exp-Session-Id`), to that run's
// persisted pick, and to a person's key (the agent's own key is refused).
// EXP-1140 follow-up: the agent's own key writes neither the registry nor a
// credential (an owner's agent could re-point a server's url and let `test`
// POST the decrypted token there). A person's key keeps every write.
describe(`mcpServers — agent-key refusals`, () => {
  const keys = [
    { id: `key-agent`, referenceId: `actor`, metadata: JSON.stringify({ kind: `agent` }) },
    { id: `key-person`, referenceId: `actor`, metadata: JSON.stringify({ kind: `personal` }) },
  ]

  it(`refuses update, test and every other write on the agent key`, async () => {
    db = createFakeDb({ mcp_servers: [serverRow({ url: MCP_URL })], apikeys: keys })
    const agent = callerFor(`actor`, { keyId: `key-agent` })
    const writes: Array<Promise<unknown>> = [
      agent.update({ id: SERVER, url: `https://evil.example.com/mcp` }),
      agent.test({ serverId: SERVER }),
      agent.create({ teamId: TEAM, name: `X`, transport: `http`, url: `https://x.example.com`, auth: `none` }),
      agent.remove({ id: SERVER }),
      agent.connect({ serverId: SERVER }),
      agent.setSecret({ serverId: SERVER, value: `v` }),
      agent.disconnect({ serverId: SERVER }),
      agent.setShared({ serverId: SERVER, shared: true }),
      agent.probe({ teamId: TEAM, url: `https://x.example.com` }),
    ]
    for (const write of writes) {
      const error = await rejectionOf(write)
      expect(error.code).toBe(`FORBIDDEN`)
      expect(error.message).toBe(`Agent keys cannot manage MCP servers or their credentials`)
    }
    // Nothing changed and the registry read still works for the agent.
    const [row] = await db.select().from((await import(`@/db/schema`)).mcpServers)
    expect(row).toMatchObject({ url: MCP_URL })
    expect(await agent.list({ teamId: TEAM })).toHaveLength(1)
  })

  it(`a person's key still writes`, async () => {
    db = createFakeDb({ mcp_servers: [serverRow({ url: MCP_URL })], apikeys: keys })
    const person = callerFor(`actor`, { keyId: `key-person` })
    const row = await person.update({ id: SERVER, name: `Linear 2` })
    expect(row.name).toBe(`Linear 2`)
  })
})

describe(`mcpServers.resolveForLaunch`, () => {
  const RUN = `99999999-9999-4999-8999-999999999999`
  const cred = (userId: string, serverId: string, token: string) => ({
    id: `c-${userId}-${serverId}`,
    serverId,
    userId,
    teamId: TEAM,
    ciphertext: encryptCredential({ accessToken: token }, credentialAad(serverId, userId)),
    expiresAt: null,
    error: null,
  })
  const run = (over: Record<string, unknown> = {}) => ({
    id: RUN,
    userId: `actor`,
    hostUserId: null,
    teamId: TEAM,
    status: `running`,
    mcpServerIds: [SERVER, SERVER_B],
    ...over,
  })
  const seed = (over: { coding_sessions?: Record<string, unknown>[]; apikeys?: Record<string, unknown>[] } = {}) => {
    db = createFakeDb({
      mcp_servers: [
        serverRow({ url: MCP_URL }),
        serverRow({ id: SERVER_B, name: `Grafana`, auth: `secret`, headerNames: [`X-Api-Key`] }),
      ],
      mcp_credentials: [cred(`actor`, SERVER, `tok`), cred(`teammate`, SERVER, `their-tok`)],
      coding_sessions: over.coding_sessions ?? [run()],
      apikeys: over.apikeys ?? [],
    })
  }
  const inRun = (userId = `actor`, keyId?: string) =>
    callerFor(userId, { sessionId: RUN, keyId })

  it(`hands the caller its own values for the run's pick, scoped to the run's team`, async () => {
    seed()
    const result = await inRun().resolveForLaunch({ serverIds: [SERVER, SERVER_B] })
    expect(result.servers).toEqual([
      expect.objectContaining({ id: SERVER, headers: [{ name: `Authorization`, value: `Bearer tok` }] }),
    ])
    expect(result.skipped).toEqual([{ id: SERVER_B, name: `Grafana`, reason: `not connected` }])
    // The run's team is the only scope — never "every team the caller is in".
    expect(h.getUserTeamIds).not.toHaveBeenCalled()
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
  })

  it(`refuses ids the run did not pick`, async () => {
    seed({ coding_sessions: [run({ mcpServerIds: [SERVER_B] })] })
    // SERVER is connected for the caller, but this run never picked it.
    const result = await inRun().resolveForLaunch({ serverIds: [SERVER, SERVER_B] })
    expect(result.servers).toEqual([])
    expect(result.skipped).toEqual([
      { id: SERVER_B, name: `Grafana`, reason: `not connected` },
      { id: SERVER, name: SERVER, reason: `not picked for this run` },
    ])
    expect(JSON.stringify(result)).not.toContain(`tok`)
  })

  it(`refuses without the session header`, async () => {
    seed()
    await expect(
      callerFor().resolveForLaunch({ serverIds: [SERVER] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(
      callerFor(`actor`, { sessionId: `not-a-uuid` }).resolveForLaunch({ serverIds: [SERVER] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`refuses another member's run, an ended run and a vanished row`, async () => {
    seed({ coding_sessions: [run({ userId: `teammate` })] })
    await expect(inRun().resolveForLaunch({ serverIds: [SERVER] })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    seed({ coding_sessions: [run({ status: `ended` })] })
    await expect(inRun().resolveForLaunch({ serverIds: [SERVER] })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    seed({ coding_sessions: [] })
    await expect(inRun().resolveForLaunch({ serverIds: [SERVER] })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
  })

  it(`resolves a shared-device host's OWN credentials for a teammate's run`, async () => {
    // The daemon calls as the device owner; the row belongs to the requester.
    seed({ coding_sessions: [run({ userId: `teammate`, hostUserId: `actor` })] })
    const result = await inRun().resolveForLaunch({ serverIds: [SERVER] })
    expect(result.servers).toEqual([
      expect.objectContaining({ id: SERVER, headers: [{ name: `Authorization`, value: `Bearer tok` }] }),
    ])
    expect(JSON.stringify(result)).not.toContain(`their-tok`)
  })

  it(`refuses the agent's own key, never a person's`, async () => {
    seed({
      apikeys: [
        { id: `key-agent`, referenceId: `actor`, metadata: JSON.stringify({ kind: `agent` }) },
        { id: `key-person`, referenceId: `actor`, metadata: JSON.stringify({ kind: `personal` }) },
        { id: `key-legacy`, referenceId: `actor`, metadata: null },
      ],
    })
    await expect(
      inRun(`actor`, `key-agent`).resolveForLaunch({ serverIds: [SERVER] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    for (const keyId of [`key-person`, `key-legacy`]) {
      const result = await inRun(`actor`, keyId).resolveForLaunch({ serverIds: [SERVER] })
      expect(result.servers).toHaveLength(1)
    }
  })

  it(`bounds the id list at 16 uuids`, async () => {
    seed()
    await expect(
      inRun().resolveForLaunch({
        serverIds: Array.from({ length: 17 }, () => SERVER),
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})

describe(`isReservedMcpEnvName`, () => {
  it(`denies the launcher's environment, case-insensitively`, () => {
    for (const name of [`PATH`, `Home`, `shell`, `USER`, `TMPDIR`, `PWD`, `LD_PRELOAD`, `DYLD_LIBRARY_PATH`, `NODE_OPTIONS`, `ANTHROPIC_API_KEY`, `claude_config_dir`, `CODEX_HOME`, `OPENAI_API_KEY`, `EXP_MCP_TOKEN_1`, `GIT_SSH_COMMAND`, `BUN_INSTALL`]) {
      expect(isReservedMcpEnvName(name), name).toBe(true)
    }
    for (const name of [`PG_URL`, `GITHUB_TOKEN`, `LINEAR_API_KEY`, `PATHS`, `HOMEBREW_X`, `EXPO_TOKEN`]) {
      expect(isReservedMcpEnvName(name), name).toBe(false)
    }
  })
})

describe(`mcpServers.setShared (FEED-73)`, () => {
  const LATER = new Date(Date.now() + 3600_000)
  const seed = (credentials: Record<string, unknown>[]) => {
    db = createFakeDb({
      mcp_servers: [serverRow(), serverRow({ id: SERVER_B, name: `Docs`, auth: `none` })],
      mcp_credentials: credentials,
    })
  }
  const own = (over: Record<string, unknown> = {}) => ({
    id: `c-actor`,
    serverId: SERVER,
    userId: `actor`,
    teamId: TEAM,
    ciphertext: encryptCredential({ accessToken: `a`, refreshToken: `r` }, credentialAad(SERVER, `actor`)),
    expiresAt: LATER,
    error: null,
    shared: false,
    ...over,
  })

  it(`toggles the caller's own connected row and returns the connection`, async () => {
    seed([own(), { ...own(), id: `c-mate`, userId: `mate`, ciphertext: encryptCredential({ accessToken: `b` }, credentialAad(SERVER, `mate`)) }])
    const on = await callerFor().setShared({ serverId: SERVER, shared: true })
    expect(on).toMatchObject({ status: `connected`, shared: true })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
    const rows = db.rows(`mcp_credentials`)
    expect(rows.find((row) => row.userId === `actor`)!.shared).toBe(true)
    // Never someone else's row.
    expect(rows.find((row) => row.userId === `mate`)!.shared).toBe(false)
    const off = await callerFor().setShared({ serverId: SERVER, shared: false })
    expect(off.shared).toBe(false)
  })

  it(`refuses without a connected row of the caller's`, async () => {
    seed([own({ error: `invalid_grant` })])
    for (const call of [
      callerFor().setShared({ serverId: SERVER, shared: true }),
      callerFor(`mate`).setShared({ serverId: SERVER, shared: true }),
      callerFor().setShared({ serverId: SERVER_B, shared: true }),
    ]) {
      const error = await rejectionOf(call)
      expect(error.code).toBe(`PRECONDITION_FAILED`)
      expect(error.message).toBe(`Connect first`)
    }
    expect(db.rows(`mcp_credentials`)[0]!.shared).toBe(false)
  })

  it(`is member-gated`, async () => {
    seed([own()])
    h.assertTeamMember.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    const error = await rejectionOf(callerFor().setShared({ serverId: SERVER, shared: true }))
    expect(error.code).toBe(`FORBIDDEN`)
    expect(db.rows(`mcp_credentials`)[0]!.shared).toBe(false)
  })
})

describe(`mcpServers.resolveForLaunch — action runs spend shared connections (FEED-73)`, () => {
  const RUN = `99999999-9999-4999-8999-999999999999`
  const ACTION = `88888888-8888-4888-8888-888888888888`
  const STDIO = `11111111-1111-4111-8111-333333333333`
  const cred = (userId: string, serverId: string, payload: object, shared = true) => ({
    id: `c-${userId}-${serverId}`,
    serverId,
    userId,
    teamId: TEAM,
    ciphertext: encryptCredential(payload, credentialAad(serverId, userId)),
    expiresAt: null,
    error: null,
    shared,
  })
  const seed = (run: Record<string, unknown>) => {
    db = createFakeDb({
      mcp_servers: [
        serverRow({ url: MCP_URL }),
        serverRow({ id: SERVER_B, name: `Grafana`, auth: `secret`, headerNames: [`X-Api-Key`] }),
        serverRow({ id: STDIO, name: `pg`, transport: `stdio`, url: null, command: `npx`, auth: `secret`, envNames: [`PG_URL`] }),
      ],
      team_members: [`actor`, `chris`, `dana`].map((userId) => ({ teamId: TEAM, userId })),
      users: [
        { id: `actor`, name: `Actor`, email: `actor@x.io` },
        { id: `chris`, name: `Chris Doe`, email: `chris@x.io` },
        { id: `dana`, name: `Dana`, email: `dana@x.io` },
      ],
      mcp_credentials: [
        cred(`actor`, SERVER, { accessToken: `own-tok` }),
        cred(`chris`, SERVER, { accessToken: `chris-tok` }),
        cred(`dana`, SERVER, { accessToken: `dana-tok` }, false),
        cred(`chris`, STDIO, { value: `postgres://chris` }),
      ],
      coding_sessions: [
        {
          id: RUN,
          userId: `actor`,
          hostUserId: null,
          teamId: TEAM,
          status: `running`,
          mcpServerIds: [SERVER, SERVER_B, STDIO],
          actionId: ACTION,
          ...run,
        },
      ],
    })
  }
  const inRun = () => callerFor(`actor`, { sessionId: RUN })

  it(`returns own entries plus other members' shared http ones, with actors and a roster`, async () => {
    seed({})
    // serverIds omitted = the row's whole pick.
    const result = await inRun().resolveForLaunch({})
    expect(result.servers.map((entry) => [entry.id, entry.name, entry.actor])).toEqual([
      [SERVER, `Linear`, { userId: `actor`, name: `Actor`, shared: false }],
      [SERVER, `Linear-as-chris`, { userId: `chris`, name: `Chris Doe`, shared: true }],
    ])
    expect(result.servers[1]!.headers).toEqual([{ name: `Authorization`, value: `Bearer chris-tok` }])
    // Never the caller's own row twice, never an unshared one, never a stdio share.
    expect(JSON.stringify(result.servers)).not.toMatch(/dana-tok|postgres/)
    expect(result.skipped).toEqual([
      { id: SERVER_B, name: `Grafana`, reason: `not connected` },
      { id: STDIO, name: `pg`, reason: `not connected` },
    ])
    expect(result.members.map((member) => `${member.serverName}:${member.userId}:${member.state}`)).toEqual([
      `Grafana:actor:self`,
      `Grafana:chris:not_connected`,
      `Grafana:dana:not_connected`,
      `Linear:actor:self`,
      `Linear:chris:shared`,
      `Linear:dana:connected`,
      `pg:actor:self`,
      `pg:chris:connected`,
      `pg:dana:not_connected`,
    ])
  })

  it(`an explicit serverIds list narrows the pick and still skips unpicked ids`, async () => {
    seed({ mcpServerIds: [SERVER] })
    const result = await inRun().resolveForLaunch({ serverIds: [SERVER, SERVER_B] })
    expect(result.servers.map((entry) => entry.name)).toEqual([`Linear`, `Linear-as-chris`])
    expect(result.skipped).toEqual([{ id: SERVER_B, name: SERVER_B, reason: `not picked for this run` }])
    expect(result.members.every((member) => member.serverId === SERVER)).toBe(true)
  })

  it(`a non-action run gets no extras and an empty roster`, async () => {
    seed({ actionId: null })
    const result = await inRun().resolveForLaunch({})
    expect(result.servers.map((entry) => entry.name)).toEqual([`Linear`])
    expect(result.members).toEqual([])
    expect(JSON.stringify(result)).not.toContain(`chris-tok`)
  })
})
