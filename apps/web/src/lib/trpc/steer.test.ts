import { beforeEach, describe, expect, it, vi } from "vitest"

// VAPP-43: the steer router mints CONTROL tickets only — the relay carries
// no session content, so there is no publisher or viewer ticket any more.

const h = vi.hoisted(() => ({
  getSteerRelayConfig: vi.fn(),
  mintSteerTicket: vi.fn((_config: unknown, seed: unknown) => ({ ticket: `t`, url: `u`, seed })),
  loadSessionForUser: vi.fn(),
  sessionDeviceOwner: vi.fn(async () => `host-owner`),
  root: [{ sealPublicKey: `k` }] as Array<{ sealPublicKey: string | null }>,
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  mintSteerTicket: h.mintSteerTicket,
}))

vi.mock(`@/lib/sessions/access`, () => ({
  loadSessionForUser: h.loadSessionForUser,
  sessionDeviceOwner: h.sessionDeviceOwner,
}))

const { steerRouter } = await import(`@/lib/trpc/steer`)

const SESSION_ID = `11111111-1111-4111-8111-111111111111`

function caller(userId: string) {
  return steerRouter.createCaller({
    session: { user: { id: userId } },
    db: { select: () => ({ from: () => ({ where: () => ({ limit: async () => h.root }) }) }) },
    request: new Request(`http://x.test`),
  } as never)
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

  it(`mints a control ticket for the caller's account`, async () => {
    await caller(`owner`).mintTicket({ kind: `control` })
    expect(h.mintSteerTicket.mock.calls[0]![1]).toEqual({ kind: `control`, userId: `owner` })
  })

  it(`refuses the removed publisher and viewer kinds`, async () => {
    for (const kind of [`publisher`, `viewer`]) {
      await expect(
        caller(`owner`).mintTicket({ kind, sessionId: SESSION_ID } as never)
      ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    }
    expect(h.mintSteerTicket).not.toHaveBeenCalled()
  })
})

describe(`steer.tunnelTicket (#EXP-1525)`, () => {
  const session = { id: SESSION_ID, rootId: `root`, teamId: `team`, deviceId: `dev-1`, userId: `owner`, shared: false }

  beforeEach(() => {
    h.loadSessionForUser.mockResolvedValue(session)
    h.root = [{ sealPublicKey: `k` }]
  })

  it(`reports disabled without a relay`, async () => {
    h.getSteerRelayConfig.mockReturnValue(null)
    expect(await caller(`owner`).tunnelTicket({ sessionId: SESSION_ID })).toEqual({ disabled: true })
  })

  it(`mints a tunnel ticket to the session's host device`, async () => {
    await caller(`viewer`).tunnelTicket({ sessionId: SESSION_ID })
    expect(h.loadSessionForUser.mock.calls[0]!.slice(0, 2)).toEqual([`viewer`, SESSION_ID])
    expect(h.mintSteerTicket.mock.calls[0]![1]).toEqual({
      kind: `tunnel`,
      userId: `viewer`,
      teamId: `team`,
      sessionId: SESSION_ID,
      deviceId: `dev-1`,
      deviceOwnerId: `host-owner`,
    })
  })

  it(`a keyless root has no tunnel: the relay only carries sealed envelopes`, async () => {
    h.root = [{ sealPublicKey: null }]
    expect(await caller(`owner`).tunnelTicket({ sessionId: SESSION_ID })).toEqual({ unavailable: true })
    expect(h.mintSteerTicket).not.toHaveBeenCalled()
  })

  it(`an invisible session is NOT_FOUND`, async () => {
    h.loadSessionForUser.mockRejectedValue(Object.assign(new Error(`nf`), { code: `NOT_FOUND` }))
    await expect(caller(`stranger`).tunnelTicket({ sessionId: SESSION_ID })).rejects.toThrow()
    expect(h.mintSteerTicket).not.toHaveBeenCalled()
  })
})
