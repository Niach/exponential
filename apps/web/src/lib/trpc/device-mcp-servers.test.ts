import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-891: the deviceMcpServers router. `sync` is the device's full-set
// replace, authorized on device ownership and refused whole on a bad row;
// `list` is member-read over the devices visible in the team, captioned per
// machine. Runs against the in-memory fake (lib/mcp-oauth/test-db.ts) on
// the real schema tables; membership and the device helpers are stubbed.

const h = vi.hoisted(() => ({
  assertTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
  visibleDeviceRows: vi.fn(async () => ({
    rows: [] as unknown[],
    ownerNames: new Map<string, { id: string; name: string }>(),
  })),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
}))
vi.mock(`@/lib/trpc/devices`, () => ({
  visibleDeviceRows: h.visibleDeviceRows,
}))

import { deviceMcpServersRouter } from "@/lib/trpc/device-mcp-servers"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"

const TEAM = `44444444-4444-4444-8444-444444444444`
const DEVICE_ROW = `22222222-2222-4222-8222-222222222222`
const OTHER_DEVICE_ROW = `22222222-2222-4222-8222-333333333333`

function deviceRow(over: Record<string, unknown> = {}) {
  return {
    id: DEVICE_ROW,
    userId: `actor`,
    deviceId: `dev-1`,
    label: `MacBook`,
    ...over,
  }
}

let db: FakeDb
function callerFor(userId = `actor`) {
  return deviceMcpServersRouter.createCaller({
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

const linear = {
  name: `linear`,
  transport: `http` as const,
  url: `https://mcp.linear.app/mcp`,
  source: `detected` as const,
  agent: `claude` as const,
  enabled: true,
}
const acme = {
  name: `acme`,
  transport: `stdio` as const,
  command: `npx`,
  args: [`-y`, `@acme/mcp`],
  source: `manual` as const,
  enabled: false,
}

beforeEach(() => {
  h.assertTeamMember.mockClear()
  h.visibleDeviceRows.mockClear()
  h.visibleDeviceRows.mockResolvedValue({ rows: [], ownerNames: new Map() })
  db = createFakeDb({ devices: [deviceRow()] })
})

describe(`deviceMcpServers.sync`, () => {
  it(`stores the machine's set under the caller's own device`, async () => {
    const result = await callerFor().sync({ deviceId: `dev-1`, servers: [linear, acme] })
    expect(result).toEqual({ ok: true, count: 2 })
    const rows = db.rows(`device_mcp_servers`)
    expect(rows).toHaveLength(2)
    expect(rows[0]).toMatchObject({
      deviceRowId: DEVICE_ROW,
      deviceId: `dev-1`,
      userId: `actor`,
      name: `linear`,
      transport: `http`,
      url: `https://mcp.linear.app/mcp`,
      command: null,
      args: [],
      source: `detected`,
      agent: `claude`,
      enabled: true,
    })
    expect(rows[1]).toMatchObject({
      name: `acme`,
      transport: `stdio`,
      url: null,
      command: `npx`,
      args: [`-y`, `@acme/mcp`],
      source: `manual`,
      agent: null,
      enabled: false,
    })
  })

  it(`replaces: upserts by name, deletes what the device no longer lists`, async () => {
    await callerFor().sync({ deviceId: `dev-1`, servers: [linear, acme] })
    await callerFor().sync({
      deviceId: `dev-1`,
      servers: [{ ...linear, enabled: false }],
    })
    const rows = db.rows(`device_mcp_servers`)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ name: `linear`, enabled: false })
    await callerFor().sync({ deviceId: `dev-1`, servers: [] })
    expect(db.rows(`device_mcp_servers`)).toHaveLength(0)
  })

  it(`leaves another device's rows alone`, async () => {
    db = createFakeDb({
      devices: [deviceRow(), deviceRow({ id: OTHER_DEVICE_ROW, deviceId: `dev-2` })],
      device_mcp_servers: [
        {
          id: `existing`,
          deviceRowId: OTHER_DEVICE_ROW,
          deviceId: `dev-2`,
          userId: `actor`,
          name: `sentry`,
          transport: `http`,
          url: `https://mcp.sentry.dev/mcp`,
          command: null,
          args: [],
          source: `manual`,
          agent: null,
          enabled: true,
          createdAt: new Date(0),
          updatedAt: new Date(0),
        },
      ],
    })
    await callerFor().sync({ deviceId: `dev-1`, servers: [linear] })
    const names = db.rows(`device_mcp_servers`).map((row) => row.name)
    expect(names.sort()).toEqual([`linear`, `sentry`])
  })

  it(`authorizes on owning the device`, async () => {
    const error = await rejectionOf(
      callerFor(`someone-else`).sync({ deviceId: `dev-1`, servers: [linear] })
    )
    expect(error.code).toBe(`NOT_FOUND`)
    expect(db.rows(`device_mcp_servers`)).toHaveLength(0)
  })

  it(`refuses the whole set on a bad row and names it`, async () => {
    const missingUrl = await rejectionOf(
      callerFor().sync({
        deviceId: `dev-1`,
        servers: [linear, { ...acme, name: `broken`, transport: `http`, command: null }],
      })
    )
    expect(missingUrl.code).toBe(`BAD_REQUEST`)
    expect(missingUrl.message).toMatch(/broken: an http server needs a URL/)
    const reserved = await rejectionOf(
      callerFor().sync({
        deviceId: `dev-1`,
        servers: [{ ...linear, name: `Exponential` }],
      })
    )
    expect(reserved.message).toMatch(/reserved/)
    const clash = await rejectionOf(
      callerFor().sync({
        deviceId: `dev-1`,
        servers: [linear, { ...linear, name: `Linear` }],
      })
    )
    expect(clash.message).toMatch(/config key linear/)
    expect(db.rows(`device_mcp_servers`)).toHaveLength(0)
  })

  // Review B1: the row is readable by teammates the device is shared with,
  // so a credential riding in the URL never gets stored.
  it(`refuses a URL carrying credentials or a query string, storing nothing`, async () => {
    for (const url of [
      `https://user:pass@mcp.example.com/mcp`,
      `https://mcp.example.com/mcp?api_key=sk-123`,
    ]) {
      const error = await rejectionOf(
        callerFor().sync({
          deviceId: `dev-1`,
          servers: [linear, { ...linear, name: `leaky`, url }],
        })
      )
      expect(error.code).toBe(`BAD_REQUEST`)
      expect(error.message).toMatch(
        /leaky: the URL must not carry a username, a password or a query string/
      )
    }
    expect(db.rows(`device_mcp_servers`)).toHaveLength(0)
  })
})

describe(`deviceMcpServers.list`, () => {
  it(`is member-read over the devices visible in the team, captioned per machine`, async () => {
    db = createFakeDb({
      devices: [deviceRow(), deviceRow({ id: OTHER_DEVICE_ROW, deviceId: `dev-2`, userId: `sam`, label: `mini` })],
    })
    await callerFor().sync({ deviceId: `dev-1`, servers: [acme] })
    await callerFor(`sam`).sync({ deviceId: `dev-2`, servers: [linear] })
    h.visibleDeviceRows.mockResolvedValue({
      rows: [deviceRow(), deviceRow({ id: OTHER_DEVICE_ROW, deviceId: `dev-2`, userId: `sam`, label: `mini` })],
      ownerNames: new Map([[`sam`, { id: `sam`, name: `Sam` }]]),
    })
    const rows = await callerFor().list({ teamId: TEAM })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
    expect(rows).toHaveLength(2)
    const mine = rows.find((row) => row.name === `acme`)!
    expect(mine).toMatchObject({
      deviceId: `dev-1`,
      deviceLabel: `MacBook`,
      ownerName: `Actor`,
      userId: `actor`,
      transport: `stdio`,
      command: `npx`,
      args: [`-y`, `@acme/mcp`],
      source: `manual`,
      agent: null,
      enabled: false,
    })
    expect(typeof mine.updatedAt).toBe(`string`)
    const theirs = rows.find((row) => row.name === `linear`)!
    expect(theirs).toMatchObject({ deviceLabel: `mini`, ownerName: `Sam`, userId: `sam` })
  })

  it(`hides rows of devices the caller cannot see`, async () => {
    await callerFor().sync({ deviceId: `dev-1`, servers: [linear] })
    h.visibleDeviceRows.mockResolvedValue({ rows: [], ownerNames: new Map() })
    expect(await callerFor().list({ teamId: TEAM })).toEqual([])
  })
})
