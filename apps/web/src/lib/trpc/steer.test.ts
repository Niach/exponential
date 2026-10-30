import { beforeEach, describe, expect, it, vi } from "vitest"

// VAPP-43: the steer router mints CONTROL tickets only — the relay carries
// no session content, so there is no publisher or viewer ticket any more.

const h = vi.hoisted(() => ({
  getSteerRelayConfig: vi.fn(),
  mintSteerTicket: vi.fn((_config: unknown, seed: unknown) => ({ ticket: `t`, url: `u`, seed })),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  mintSteerTicket: h.mintSteerTicket,
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
