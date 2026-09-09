import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-792: the flow-state rules and the devices.completeCommand hook that
// advances an `mcp_oauth_*` flow. Runs against the in-memory fake db
// (test-db.ts) on the real schema tables.

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

import {
  applyMcpOauthCommandCompletion,
  defaultRedirect,
  finishFlow,
  flowAcceptsCode,
  flowIsPending,
  hostedCallbackUrl,
  isPublicHttpsBase,
  newFlowState,
  parseCompletionMessage,
} from "@/lib/mcp-oauth/flows"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"

const SERVER = `11111111-1111-4111-8111-111111111111`
const DEVICE_ROW = `22222222-2222-4222-8222-222222222222`
const FLOW = `33333333-3333-4333-8333-333333333333`

function flowRow(over: Record<string, unknown> = {}) {
  return {
    id: FLOW,
    state: `st-1`,
    userId: `owner`,
    teamId: `team-1`,
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

describe(`flow state rules`, () => {
  it(`mints a 32-byte base64url state`, () => {
    const state = newFlowState()
    expect(state).toMatch(/^[A-Za-z0-9_-]{43}$/)
    expect(newFlowState()).not.toBe(state)
  })

  it(`accepts a code only before relay and inside the TTL`, () => {
    const now = new Date(`2026-09-09T12:00:00Z`)
    const fresh = new Date(now.getTime() - 60_000)
    const stale = new Date(now.getTime() - 11 * 60_000)
    expect(flowAcceptsCode({ status: `pending`, createdAt: fresh }, now)).toBe(true)
    expect(
      flowAcceptsCode({ status: `authorize_url`, createdAt: fresh }, now)
    ).toBe(true)
    expect(
      flowAcceptsCode({ status: `code_relayed`, createdAt: fresh }, now)
    ).toBe(false)
    expect(flowAcceptsCode({ status: `done`, createdAt: fresh }, now)).toBe(false)
    expect(flowAcceptsCode({ status: `pending`, createdAt: stale }, now)).toBe(false)
  })

  it(`treats a live flow past the TTL as no longer pending`, () => {
    const now = new Date(`2026-09-09T12:00:00Z`)
    const stale = new Date(now.getTime() - 10 * 60_000)
    expect(flowIsPending({ status: `code_relayed`, createdAt: now }, now)).toBe(true)
    expect(flowIsPending({ status: `pending`, createdAt: stale }, now)).toBe(false)
    expect(flowIsPending({ status: `failed`, createdAt: now }, now)).toBe(false)
  })

  it(`picks the hosted callback only for a public https base`, () => {
    expect(isPublicHttpsBase(`https://app.exponential.dev`)).toBe(true)
    expect(defaultRedirect(`https://app.exponential.dev`)).toBe(`hosted`)
    for (const base of [
      `http://app.exponential.dev`,
      `https://localhost:3000`,
      `https://127.0.0.1`,
      `https://dev.localhost`,
      `https://192.168.178.111`,
      `https://10.0.0.5`,
      `https://172.20.1.1`,
      `https://[::1]:3000`,
      `not a url`,
    ]) {
      expect(isPublicHttpsBase(base), base).toBe(false)
      expect(defaultRedirect(base), base).toBe(`loopback`)
    }
    expect(hostedCallbackUrl(`https://app.exponential.dev/`)).toBe(
      `https://app.exponential.dev/api/mcp-oauth/callback`
    )
  })

  it(`parses completion messages defensively`, () => {
    expect(parseCompletionMessage(undefined)).toBeNull()
    expect(parseCompletionMessage(`not json`)).toBeNull()
    expect(parseCompletionMessage(`[1]`)).toBeNull()
    expect(parseCompletionMessage(`{"phase":"done"}`)).toEqual({ phase: `done` })
  })
})

describe(`applyMcpOauthCommandCompletion`, () => {
  let db: FakeDb
  beforeEach(() => {
    db = createFakeDb({ mcp_oauth_flows: [flowRow()] })
  })
  const flow = () => db.rows(`mcp_oauth_flows`)[0]!
  const readiness = () => db.rows(`mcp_server_readiness`)

  it(`mcp_oauth_start ok with an authorize url → authorize_url`, async () => {
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `mcp_oauth_start`,
      payload: { serverId: SERVER, state: `st-1`, redirectUri: `loopback` },
      ok: true,
      message: JSON.stringify({
        phase: `authorize`,
        url: `https://as.example.com/authorize?x=1`,
      }),
    })
    expect(flow()).toMatchObject({
      status: `authorize_url`,
      authorizeUrl: `https://as.example.com/authorize?x=1`,
    })
  })

  it(`mcp_oauth_start ok with a non-JSON message fails the flow`, async () => {
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `mcp_oauth_start`,
      payload: { state: `st-1` },
      ok: true,
      message: `Opened the browser`,
    })
    expect(flow()).toMatchObject({
      status: `failed`,
      error: `device returned no authorize url`,
    })
    expect(flow().completedAt).toBeInstanceOf(Date)
  })

  it(`mcp_oauth_start ok=false fails the flow with the device's reason`, async () => {
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `mcp_oauth_start`,
      payload: { state: `st-1` },
      ok: false,
      message: `provider requires a pre-registered client`,
    })
    expect(flow()).toMatchObject({
      status: `failed`,
      error: `provider requires a pre-registered client`,
    })
  })

  it(`mcp_oauth_code ok → done + readiness ready with the expiry`, async () => {
    db = createFakeDb({
      mcp_oauth_flows: [flowRow({ status: `code_relayed` })],
    })
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `mcp_oauth_code`,
      payload: { serverId: SERVER, state: `st-1`, code: `c` },
      ok: true,
      message: JSON.stringify({
        phase: `done`,
        expiresAt: `2026-09-09T13:00:00Z`,
      }),
    })
    expect(flow()).toMatchObject({ status: `done`, error: null })
    expect(readiness()).toHaveLength(1)
    expect(readiness()[0]).toMatchObject({
      serverId: SERVER,
      deviceRowId: DEVICE_ROW,
      userId: `owner`,
      ready: true,
      expiresAt: new Date(`2026-09-09T13:00:00Z`),
      error: null,
    })
  })

  it(`mcp_oauth_code ok=false → failed + readiness not ready`, async () => {
    db = createFakeDb({
      mcp_oauth_flows: [flowRow({ status: `code_relayed` })],
      mcp_server_readiness: [
        {
          id: `r-1`,
          serverId: SERVER,
          deviceRowId: DEVICE_ROW,
          userId: `owner`,
          ready: true,
          expiresAt: null,
          error: null,
          checkedAt: new Date(0),
        },
      ],
    })
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `mcp_oauth_code`,
      payload: { state: `st-1` },
      ok: false,
      message: `token endpoint answered 400`,
    })
    expect(flow()).toMatchObject({
      status: `failed`,
      error: `token endpoint answered 400`,
    })
    // Upsert, not a second row.
    expect(readiness()).toHaveLength(1)
    expect(readiness()[0]).toMatchObject({
      ready: false,
      error: `token endpoint answered 400`,
    })
  })

  it(`ignores other kinds, unknown states and terminal flows`, async () => {
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `worktree_prune`,
      payload: { state: `st-1` },
      ok: true,
    })
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `mcp_oauth_code`,
      payload: { state: `nope` },
      ok: true,
    })
    expect(flow().status).toBe(`pending`)
    await finishFlow(db as never, FLOW, { ok: false, error: `cancelled` })
    await applyMcpOauthCommandCompletion(db as never, {
      kind: `mcp_oauth_code`,
      payload: { state: `st-1` },
      ok: true,
    })
    expect(flow()).toMatchObject({ status: `failed`, error: `cancelled` })
    expect(readiness()).toHaveLength(0)
  })

  it(`finishFlow moves live rows only`, async () => {
    await finishFlow(db as never, FLOW, { ok: true })
    expect(flow().status).toBe(`done`)
    await finishFlow(db as never, FLOW, { ok: false, error: `late` })
    expect(flow()).toMatchObject({ status: `done`, error: null })
  })
})
