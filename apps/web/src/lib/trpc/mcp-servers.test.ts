import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-792: the mcpServers router. Owner-only writes with the cross-field
// validation matrix, member reads with readiness limited to visible devices,
// and the web-initiated / device-executed OAuth flow (beginOAuth queues the
// device command, getOAuthFlow ages a flow out, finishOAuth authorizes on
// device ownership). The router runs against ctx.db, so the in-memory fake
// (lib/mcp-oauth/test-db.ts) on the real schema tables is enough; membership
// and the device helpers are stubbed.

const h = vi.hoisted(() => ({
  assertTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
  assertTeamOwner: vi.fn(async () => ({ role: `owner` }) as unknown),
  getUserTeamIds: vi.fn(async () => [`team-1`]),
  nudgeDevice: vi.fn(),
  visibleDeviceRows: vi.fn(async () => ({
    rows: [] as unknown[],
    ownerNames: new Map(),
  })),
  appBaseUrl: vi.fn(() => `https://app.exponential.dev`),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  assertTeamOwner: h.assertTeamOwner,
  getUserTeamIds: h.getUserTeamIds,
}))
vi.mock(`@/lib/trpc/devices`, () => ({
  nudgeDevice: h.nudgeDevice,
  visibleDeviceRows: h.visibleDeviceRows,
}))
vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: h.appBaseUrl,
}))

import { mcpServersRouter } from "@/lib/trpc/mcp-servers"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"

const TEAM = `44444444-4444-4444-8444-444444444444`
const SERVER = `11111111-1111-4111-8111-111111111111`
const SERVER_B = `11111111-1111-4111-8111-222222222222`
const DEVICE_ROW = `22222222-2222-4222-8222-222222222222`
const OTHER_DEVICE_ROW = `22222222-2222-4222-8222-333333333333`
const FLOW = `33333333-3333-4333-8333-333333333333`

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

function deviceRow(over: Record<string, unknown> = {}) {
  return {
    id: DEVICE_ROW,
    userId: `actor`,
    deviceId: `dev-1`,
    label: `MacBook`,
    caps: [`mcp`, `agent-usage-refresh`],
    ...over,
  }
}

function flowRow(over: Record<string, unknown> = {}) {
  return {
    id: FLOW,
    state: `st-1`,
    userId: `actor`,
    teamId: TEAM,
    serverId: SERVER,
    deviceRowId: DEVICE_ROW,
    redirect: `hosted`,
    status: `pending`,
    authorizeUrl: null,
    error: null,
    createdAt: new Date(),
    completedAt: null,
    ...over,
  }
}

let db: FakeDb
function callerFor(userId = `actor`) {
  return mcpServersRouter.createCaller({
    session: { user: { id: userId, name: `Actor`, email: `a@example.com` } },
    db,
    request: new Request(`http://localhost/`),
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
  h.nudgeDevice.mockClear()
  h.visibleDeviceRows.mockClear()
  h.visibleDeviceRows.mockResolvedValue({ rows: [], ownerNames: new Map() })
  h.appBaseUrl.mockReturnValue(`https://app.exponential.dev`)
  db = createFakeDb()
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
      callerFor().update({ id: FLOW, name: `x` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    h.assertTeamOwner.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(callerFor().remove({ id: SERVER })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    expect(db.rows(`mcp_servers`)).toHaveLength(2)
  })

  it(`remove deletes the row`, async () => {
    await expect(callerFor().remove({ id: SERVER })).resolves.toEqual({
      ok: true,
    })
    expect(db.rows(`mcp_servers`).map((r) => r.id)).toEqual([SERVER_B])
  })
})

describe(`mcpServers.list / listForDevice`, () => {
  beforeEach(() => {
    db = createFakeDb({
      mcp_servers: [
        serverRow({ name: `Linear` }),
        serverRow({ id: SERVER_B, name: `GitHub`, teamId: `team-2` }),
      ],
      mcp_server_readiness: [
        {
          id: `r-1`,
          serverId: SERVER,
          deviceRowId: DEVICE_ROW,
          userId: `actor`,
          ready: true,
          expiresAt: new Date(`2026-09-09T13:00:00Z`),
          error: null,
          checkedAt: new Date(`2026-09-09T12:00:00Z`),
        },
        // A device the caller cannot see: never listed.
        {
          id: `r-2`,
          serverId: SERVER,
          deviceRowId: OTHER_DEVICE_ROW,
          userId: `stranger`,
          ready: true,
          expiresAt: null,
          error: null,
          checkedAt: new Date(`2026-09-09T12:00:00Z`),
        },
      ],
    })
  })

  it(`list joins readiness for visible devices only`, async () => {
    h.visibleDeviceRows.mockResolvedValue({
      rows: [deviceRow()],
      ownerNames: new Map(),
    })
    const rows = await callerFor().list({ teamId: TEAM })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
    expect(h.visibleDeviceRows).toHaveBeenCalledWith(db, `actor`, TEAM)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ id: SERVER, name: `Linear` })
    expect(rows[0]!.readiness).toEqual([
      {
        serverId: SERVER,
        deviceRowId: DEVICE_ROW,
        deviceId: `dev-1`,
        deviceLabel: `MacBook`,
        userId: `actor`,
        ready: true,
        expiresAt: `2026-09-09T13:00:00.000Z`,
        error: null,
        checkedAt: `2026-09-09T12:00:00.000Z`,
      },
    ])
  })

  it(`list with no visible device carries empty readiness`, async () => {
    const rows = await callerFor().list({ teamId: TEAM })
    expect(rows[0]!.readiness).toEqual([])
  })

  it(`listForDevice spans every team of the caller`, async () => {
    h.getUserTeamIds.mockResolvedValue([TEAM, `team-2`])
    const rows = await callerFor().listForDevice()
    expect(rows.map((r) => r.id).sort()).toEqual([SERVER, SERVER_B].sort())
    h.getUserTeamIds.mockResolvedValue([])
    expect(await callerFor().listForDevice()).toEqual([])
  })
})

describe(`mcpServers.beginOAuth`, () => {
  beforeEach(() => {
    db = createFakeDb({
      mcp_servers: [serverRow()],
      devices: [deviceRow()],
    })
  })

  it(`creates the flow + the mcp_oauth_start command, hosted on a public https base`, async () => {
    const result = await callerFor().beginOAuth({
      serverId: SERVER,
      deviceId: `dev-1`,
    })
    expect(result).toMatchObject({ redirect: `hosted`, existing: false })
    expect(result.state).toMatch(/^[A-Za-z0-9_-]{43}$/)
    const [flow] = db.rows(`mcp_oauth_flows`)
    expect(flow).toMatchObject({
      id: result.flowId,
      state: result.state,
      userId: `actor`,
      teamId: TEAM,
      serverId: SERVER,
      deviceRowId: DEVICE_ROW,
      redirect: `hosted`,
      status: `pending`,
    })
    const [command] = db.rows(`device_commands`)
    expect(command).toMatchObject({
      deviceRowId: DEVICE_ROW,
      userId: `actor`,
      kind: `mcp_oauth_start`,
      status: `pending`,
      payload: {
        serverId: SERVER,
        state: result.state,
        redirectUri: `https://app.exponential.dev/api/mcp-oauth/callback`,
      },
    })
    expect(h.nudgeDevice).toHaveBeenCalledWith(`actor`, `dev-1`)
  })

  it(`falls back to loopback on a LAN / plain-http base, honours an explicit choice`, async () => {
    h.appBaseUrl.mockReturnValue(`http://192.168.178.111:3000`)
    const result = await callerFor().beginOAuth({
      serverId: SERVER,
      deviceId: `dev-1`,
    })
    expect(result.redirect).toBe(`loopback`)
    expect(db.rows(`device_commands`)[0]!.payload).toMatchObject({
      redirectUri: `loopback`,
    })

    db = createFakeDb({ mcp_servers: [serverRow()], devices: [deviceRow()] })
    const forced = await callerFor().beginOAuth({
      serverId: SERVER,
      deviceId: `dev-1`,
      redirect: `hosted`,
    })
    expect(forced.redirect).toBe(`hosted`)
  })

  it(`returns the pending flow instead of a duplicate`, async () => {
    const first = await callerFor().beginOAuth({
      serverId: SERVER,
      deviceId: `dev-1`,
    })
    const again = await callerFor().beginOAuth({
      serverId: SERVER,
      deviceId: `dev-1`,
    })
    expect(again).toEqual({
      flowId: first.flowId,
      state: first.state,
      redirect: `hosted`,
      existing: true,
    })
    expect(db.rows(`mcp_oauth_flows`)).toHaveLength(1)
    expect(db.rows(`device_commands`)).toHaveLength(1)
  })

  it(`mints a new flow once the pending one expired or finished`, async () => {
    db = createFakeDb({
      mcp_servers: [serverRow()],
      devices: [deviceRow()],
      mcp_oauth_flows: [
        flowRow({ createdAt: new Date(Date.now() - 11 * 60_000) }),
        flowRow({ id: `f-2`, state: `st-2`, status: `failed` }),
      ],
    })
    const result = await callerFor().beginOAuth({
      serverId: SERVER,
      deviceId: `dev-1`,
    })
    expect(result.existing).toBe(false)
    expect(db.rows(`mcp_oauth_flows`)).toHaveLength(3)
  })

  it(`refuses a non-oauth server, a device without the mcp cap, a foreign device`, async () => {
    db = createFakeDb({
      mcp_servers: [serverRow({ auth: `secret`, headerNames: [`X`] })],
      devices: [deviceRow()],
    })
    await expect(
      callerFor().beginOAuth({ serverId: SERVER, deviceId: `dev-1` })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })

    db = createFakeDb({
      mcp_servers: [serverRow()],
      devices: [deviceRow({ caps: [`agent-login`] })],
    })
    await expect(
      callerFor().beginOAuth({ serverId: SERVER, deviceId: `dev-1` })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })

    db = createFakeDb({
      mcp_servers: [serverRow()],
      devices: [deviceRow({ userId: `someone-else` })],
    })
    await expect(
      callerFor().beginOAuth({ serverId: SERVER, deviceId: `dev-1` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(db.rows(`mcp_oauth_flows`)).toHaveLength(0)
    expect(h.nudgeDevice).not.toHaveBeenCalled()
  })

  it(`requires membership of the server's team`, async () => {
    h.assertTeamMember.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN` }))
    await expect(
      callerFor().beginOAuth({ serverId: SERVER, deviceId: `dev-1` })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })
})

describe(`mcpServers.getOAuthFlow / cancelOAuth`, () => {
  it(`reports the flow, ageing an unfinished one out as failed/expired`, async () => {
    db = createFakeDb({
      mcp_oauth_flows: [
        flowRow({
          status: `authorize_url`,
          authorizeUrl: `https://as.example.com/authorize`,
        }),
      ],
    })
    expect(await callerFor().getOAuthFlow({ flowId: FLOW })).toMatchObject({
      id: FLOW,
      serverId: SERVER,
      deviceRowId: DEVICE_ROW,
      redirect: `hosted`,
      status: `authorize_url`,
      authorizeUrl: `https://as.example.com/authorize`,
      error: null,
    })

    db = createFakeDb({
      mcp_oauth_flows: [
        flowRow({ createdAt: new Date(Date.now() - 10 * 60_000) }),
      ],
    })
    expect(await callerFor().getOAuthFlow({ flowId: FLOW })).toMatchObject({
      status: `failed`,
      error: `expired`,
    })

    // A finished flow never ages out.
    db = createFakeDb({
      mcp_oauth_flows: [
        flowRow({ status: `done`, createdAt: new Date(Date.now() - 60 * 60_000) }),
      ],
    })
    expect(await callerFor().getOAuthFlow({ flowId: FLOW })).toMatchObject({
      status: `done`,
      error: null,
    })
  })

  it(`is the flow user's only`, async () => {
    db = createFakeDb({ mcp_oauth_flows: [flowRow({ userId: `someone-else` })] })
    await expect(
      callerFor().getOAuthFlow({ flowId: FLOW })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(
      callerFor().cancelOAuth({ flowId: FLOW })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`cancel fails a live flow and leaves a finished one alone`, async () => {
    db = createFakeDb({ mcp_oauth_flows: [flowRow()] })
    await expect(callerFor().cancelOAuth({ flowId: FLOW })).resolves.toEqual({
      ok: true,
    })
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      status: `failed`,
      error: `cancelled`,
    })
    db = createFakeDb({ mcp_oauth_flows: [flowRow({ status: `done` })] })
    await callerFor().cancelOAuth({ flowId: FLOW })
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`done`)
  })

  it(`closes the pending command so no browser opens for a cancelled sign-in`, async () => {
    // Commands are only ever picked up, never expired: a cancel that left the
    // row pending would still start a sign-in on the machine minutes later.
    db = createFakeDb({
      mcp_oauth_flows: [flowRow()],
      device_commands: [
        {
          id: `cmd-1`,
          deviceRowId: DEVICE_ROW,
          userId: `actor`,
          kind: `mcp_oauth_start`,
          payload: { serverId: SERVER, state: `st-1`, redirectUri: `loopback` },
          status: `pending`,
          result: null,
          completedAt: null,
          createdAt: new Date(),
          updatedAt: new Date(),
        },
        {
          id: `cmd-other`,
          deviceRowId: DEVICE_ROW,
          userId: `actor`,
          kind: `mcp_oauth_start`,
          payload: { serverId: SERVER, state: `another-flow` },
          status: `pending`,
          result: null,
          completedAt: null,
          createdAt: new Date(),
          updatedAt: new Date(),
        },
      ],
    })
    await callerFor().cancelOAuth({ flowId: FLOW })
    const rows = db.rows(`device_commands`)
    expect(rows.find((row) => row.id === `cmd-1`)).toMatchObject({
      status: `failed`,
    })
    expect(rows.find((row) => row.id === `cmd-other`)!.status).toBe(`pending`)
  })
})

describe(`mcpServers.finishOAuth`, () => {
  beforeEach(() => {
    db = createFakeDb({
      mcp_oauth_flows: [flowRow({ status: `authorize_url` })],
      devices: [deviceRow()],
    })
  })

  it(`ok → done + readiness ready (loopback path, no code command)`, async () => {
    await expect(
      callerFor().finishOAuth({
        state: `st-1`,
        ok: true,
        expiresAt: `2026-09-09T13:00:00Z`,
      })
    ).resolves.toEqual({ ok: true })
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({ status: `done` })
    expect(db.rows(`mcp_server_readiness`)).toHaveLength(1)
    expect(db.rows(`mcp_server_readiness`)[0]).toMatchObject({
      serverId: SERVER,
      deviceRowId: DEVICE_ROW,
      userId: `actor`,
      ready: true,
      expiresAt: new Date(`2026-09-09T13:00:00Z`),
    })
    // Idempotent after the code command already closed it.
    await callerFor().finishOAuth({ state: `st-1`, ok: false, error: `late` })
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      status: `done`,
      error: null,
    })
  })

  it(`ok=false → failed with the device's reason, readiness untouched`, async () => {
    await callerFor().finishOAuth({
      state: `st-1`,
      ok: false,
      error: `user closed the browser`,
    })
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      status: `failed`,
      error: `user closed the browser`,
    })
    expect(db.rows(`mcp_server_readiness`)).toHaveLength(0)
  })

  it(`only the device owner may finish; unknown state 404s`, async () => {
    await expect(
      callerFor(`someone-else`).finishOAuth({ state: `st-1`, ok: true })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(
      callerFor().finishOAuth({ state: `nope`, ok: true })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`authorize_url`)
  })
})

describe(`mcpServers.reportReadiness`, () => {
  beforeEach(() => {
    db = createFakeDb({
      mcp_servers: [
        serverRow(),
        serverRow({ id: SERVER_B, name: `Foreign`, teamId: `team-9` }),
      ],
      devices: [deviceRow()],
    })
  })

  it(`upserts the caller's team servers and ignores foreign ids`, async () => {
    await expect(
      callerFor().reportReadiness({
        deviceId: `dev-1`,
        entries: [
          { serverId: SERVER, ready: false, error: `no credential` },
          { serverId: SERVER, ready: true, expiresAt: `2026-09-09T13:00:00Z` },
          { serverId: SERVER_B, ready: true },
        ],
      })
    ).resolves.toEqual({ ok: true })
    const rows = db.rows(`mcp_server_readiness`)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({
      serverId: SERVER,
      deviceRowId: DEVICE_ROW,
      userId: `actor`,
      ready: true,
      expiresAt: new Date(`2026-09-09T13:00:00Z`),
      error: null,
    })
    await callerFor().reportReadiness({
      deviceId: `dev-1`,
      entries: [{ serverId: SERVER, ready: false, error: `refresh failed` }],
    })
    expect(db.rows(`mcp_server_readiness`)).toHaveLength(1)
    expect(db.rows(`mcp_server_readiness`)[0]).toMatchObject({
      ready: false,
      error: `refresh failed`,
      expiresAt: null,
    })
  })

  it(`needs the caller's own device`, async () => {
    await expect(
      callerFor(`someone-else`).reportReadiness({
        deviceId: `dev-1`,
        entries: [{ serverId: SERVER, ready: true }],
      })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})
