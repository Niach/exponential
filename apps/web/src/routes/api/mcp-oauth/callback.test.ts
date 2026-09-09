import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-792: the anonymous OAuth callback. State-gated and single-use, TTL
// bound, provider errors fail the flow, a good code becomes an
// `mcp_oauth_code` device command (never logged), and every refusal is the
// same plain "expired" page. The db is the in-memory fake on the real
// schema tables; the device nudge is stubbed.

const h = vi.hoisted(() => ({
  nudgeDevice: vi.fn(),
  db: null as unknown,
}))

vi.mock(`@/db/connection`, () => ({
  get db() {
    return h.db
  },
}))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/trpc/devices`, () => ({ nudgeDevice: h.nudgeDevice }))

import {
  handleMcpOauthCallback,
  resetMcpOauthCallbackLimiter,
} from "@/lib/mcp-oauth/callback"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"

const SERVER = `11111111-1111-4111-8111-111111111111`
const DEVICE_ROW = `22222222-2222-4222-8222-222222222222`
const FLOW = `33333333-3333-4333-8333-333333333333`

function seed(flowOver: Record<string, unknown> = {}): FakeDb {
  return createFakeDb({
    mcp_oauth_flows: [
      {
        id: FLOW,
        state: `st-1`,
        userId: `owner`,
        teamId: `team-1`,
        serverId: SERVER,
        deviceRowId: DEVICE_ROW,
        redirect: `hosted`,
        status: `authorize_url`,
        authorizeUrl: `https://as.example.com/authorize`,
        error: null,
        createdAt: new Date(),
        completedAt: null,
        ...flowOver,
      },
    ],
    devices: [{ id: DEVICE_ROW, userId: `owner`, deviceId: `dev-1` }],
    mcp_servers: [{ id: SERVER, name: `Linear <Prod>` }],
  })
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

let db: FakeDb
beforeEach(() => {
  resetMcpOauthCallbackLimiter()
  h.nudgeDevice.mockClear()
  db = seed()
  h.db = db
})

describe(`GET /api/mcp-oauth/callback`, () => {
  it(`relays the code to the device as an mcp_oauth_code command`, async () => {
    const res = await handleMcpOauthCallback(
      request({ code: `SECRET-CODE`, state: `st-1` })
    )
    expect(res.status).toBe(200)
    expect(res.headers.get(`content-type`)).toContain(`text/html`)
    expect(res.headers.get(`cache-control`)).toBe(`no-store`)
    const html = await res.text()
    expect(html).toContain(`Signed in to Linear &lt;Prod&gt;. You can close this tab.`)
    expect(html).not.toContain(`SECRET-CODE`)

    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      status: `code_relayed`,
    })
    expect(db.rows(`device_commands`)).toHaveLength(1)
    expect(db.rows(`device_commands`)[0]).toMatchObject({
      deviceRowId: DEVICE_ROW,
      userId: `owner`,
      kind: `mcp_oauth_code`,
      status: `pending`,
      payload: { serverId: SERVER, state: `st-1`, code: `SECRET-CODE` },
    })
    expect(h.nudgeDevice).toHaveBeenCalledWith(`owner`, `dev-1`)
  })

  it(`is single-use: a replayed redirect gets the expired page`, async () => {
    await handleMcpOauthCallback(request({ code: `c1`, state: `st-1` }))
    const res = await handleMcpOauthCallback(
      request({ code: `c2`, state: `st-1` })
    )
    expect(res.status).toBe(200)
    expect(await res.text()).toContain(`This sign-in link has expired.`)
    expect(db.rows(`device_commands`)).toHaveLength(1)
    expect(db.rows(`device_commands`)[0]!.payload).toMatchObject({ code: `c1` })
  })

  it(`refuses an unknown, missing or over-long state with the same page`, async () => {
    for (const query of [
      { code: `c`, state: `nope` },
      { code: `c` },
      { code: `c`, state: `x`.repeat(65) },
    ]) {
      const res = await handleMcpOauthCallback(request(query))
      expect(res.status).toBe(200)
      expect(await res.text()).toContain(`This sign-in link has expired.`)
    }
    expect(db.rows(`device_commands`)).toHaveLength(0)
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`authorize_url`)
    expect(h.nudgeDevice).not.toHaveBeenCalled()
  })

  it(`refuses a flow past the 10-minute TTL and a finished one`, async () => {
    h.db = db = seed({ createdAt: new Date(Date.now() - 10 * 60_000) })
    let res = await handleMcpOauthCallback(request({ code: `c`, state: `st-1` }))
    expect(await res.text()).toContain(`This sign-in link has expired.`)
    expect(db.rows(`device_commands`)).toHaveLength(0)

    h.db = db = seed({ status: `done` })
    res = await handleMcpOauthCallback(request({ code: `c`, state: `st-1` }))
    expect(await res.text()).toContain(`This sign-in link has expired.`)
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`done`)
  })

  it(`a provider error fails the flow and says why`, async () => {
    const res = await handleMcpOauthCallback(
      request({
        state: `st-1`,
        error: `access_denied`,
        error_description: `User <cancelled>`,
      })
    )
    expect(res.status).toBe(200)
    expect(await res.text()).toContain(
      `Sign-in was refused: access_denied: User &lt;cancelled&gt;`
    )
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      status: `failed`,
      error: `access_denied: User <cancelled>`,
    })
    expect(db.rows(`device_commands`)).toHaveLength(0)
    expect(h.nudgeDevice).not.toHaveBeenCalled()
  })

  it(`a redirect without a code fails the flow too`, async () => {
    const res = await handleMcpOauthCallback(request({ state: `st-1` }))
    expect(await res.text()).toContain(`Sign-in was refused: no code`)
    expect(db.rows(`mcp_oauth_flows`)[0]).toMatchObject({
      status: `failed`,
      error: `no code`,
    })
  })

  it(`rate limits per IP (burst 20) with a 429 and no db work`, async () => {
    for (let i = 0; i < 20; i += 1) {
      const res = await handleMcpOauthCallback(request({ state: `nope` }))
      expect(res.status).toBe(200)
    }
    const limited = await handleMcpOauthCallback(request({ code: `c`, state: `st-1` }))
    expect(limited.status).toBe(429)
    expect(limited.headers.get(`retry-after`)).toMatch(/^\d+$/)
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`authorize_url`)
    // Another address is unaffected.
    const other = await handleMcpOauthCallback(
      request({ code: `c`, state: `st-1` }, `198.51.100.9`)
    )
    expect(other.status).toBe(200)
    expect(db.rows(`mcp_oauth_flows`)[0]!.status).toBe(`code_relayed`)
  })
})
