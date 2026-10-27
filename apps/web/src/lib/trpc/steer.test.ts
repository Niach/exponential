import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-1291: the steer router is the live layer only — config + tickets.
// Viewing follows the session visibility rule (loadSessionForUser), publishing
// is the device account's alone.

const h = vi.hoisted(() => ({
  getSteerRelayConfig: vi.fn(),
  mintSteerTicket: vi.fn((_config: unknown, seed: unknown) => ({ ticket: `t`, url: `u`, seed })),
  loadSessionForUser: vi.fn(),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  mintSteerTicket: h.mintSteerTicket,
  viewerHistoryClaims: (await importOriginal<{ viewerHistoryClaims: unknown }>()).viewerHistoryClaims,
}))
vi.mock(`@/lib/sessions/access`, () => ({
  loadSessionForUser: h.loadSessionForUser,
  sessionDeviceOwner: (s: { userId: string; hostUserId: string | null }) =>
    s.hostUserId ?? s.userId,
}))

const { steerRouter } = await import(`@/lib/trpc/steer`)

const SESSION_ID = `11111111-1111-4111-8111-111111111111`

function caller(userId: string) {
  return steerRouter.createCaller({
    session: { user: { id: userId } },
    db: {},
    request: new Request(`http://x.test`),
  } as never)
}

function session(overrides: Record<string, unknown> = {}) {
  return {
    id: SESSION_ID,
    teamId: `team-1`,
    userId: `starter`,
    hostUserId: null,
    deviceId: `dev-1`,
    status: `running`,
    shared: true,
    ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  h.getSteerRelayConfig.mockReturnValue({ url: `wss://relay`, secret: `s` })
})

describe(`steer.mintTicket`, () => {
  it(`reports disabled without a relay`, async () => {
    h.getSteerRelayConfig.mockReturnValue(null)
    expect(await caller(`u`).mintTicket({ kind: `control` })).toEqual({ disabled: true })
  })

  it(`lets a teammate VIEW a shared session`, async () => {
    h.loadSessionForUser.mockResolvedValue(session())
    await caller(`teammate`).mintTicket({ kind: `viewer`, sessionId: SESSION_ID })
    expect(h.mintSteerTicket.mock.calls[0]![1]).toEqual({
      kind: `viewer`,
      userId: `teammate`,
      teamId: `team-1`,
      sessionId: SESSION_ID,
    })
  })

  it(`names the device on an ended session's viewer ticket`, async () => {
    h.loadSessionForUser.mockResolvedValue(session({ status: `ended`, hostUserId: `host` }))
    await caller(`starter`).mintTicket({ kind: `viewer`, sessionId: SESSION_ID })
    expect(h.mintSteerTicket.mock.calls[0]![1]).toMatchObject({
      deviceId: `dev-1`,
      deviceOwnerId: `host`,
    })
  })

  it(`names the device as live on an idle session's viewer ticket (#EXP-1405)`, async () => {
    h.loadSessionForUser.mockResolvedValue(
      session({ status: `idle`, busy: false, startedAt: new Date(`2026-10-27T10:00:00Z`), hostUserId: `host` })
    )
    await caller(`teammate`).mintTicket({ kind: `viewer`, sessionId: SESSION_ID })
    expect(h.mintSteerTicket.mock.calls[0]![1]).toMatchObject({
      deviceId: `dev-1`,
      deviceOwnerId: `host`,
      live: true,
    })
  })

  it(`refuses a publisher ticket to anyone but the device's account`, async () => {
    h.loadSessionForUser.mockResolvedValue(session({ hostUserId: `host` }))
    await expect(
      caller(`starter`).mintTicket({ kind: `publisher`, sessionId: SESSION_ID })
    ).rejects.toBeInstanceOf(TRPCError)
    await caller(`host`).mintTicket({ kind: `publisher`, sessionId: SESSION_ID })
    expect(h.mintSteerTicket).toHaveBeenCalledTimes(1)
  })

  it(`propagates the visibility refusal`, async () => {
    h.loadSessionForUser.mockRejectedValue(new TRPCError({ code: `NOT_FOUND` }))
    await expect(
      caller(`stranger`).mintTicket({ kind: `viewer`, sessionId: SESSION_ID })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})
