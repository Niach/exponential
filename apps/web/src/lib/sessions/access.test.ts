import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))
const getTeamMember = vi.fn()
vi.mock(`@/lib/auth/membership`, () => ({ getTeamMember: (...args: unknown[]) => getTeamMember(...args) }))
const resolveSessionHost = vi.fn()
vi.mock(`@/lib/sessions/host`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/sessions/host")>()
  return {
    ...actual,
    resolveSessionHost: (...args: unknown[]) => resolveSessionHost(...args),
    sessionDeviceOwner: async (session: { userId: string }) =>
      (await resolveSessionHost(session)) ?? session.userId,
    assertDeviceHost: async (userId: string, session: { userId: string }) => {
      if (((await resolveSessionHost(session)) ?? session.userId) !== userId) throw new Error(`FORBIDDEN`)
    },
  }
})

const { assertCanSteer, assertDeviceHost, sessionDeviceOwner, sessionVisibleTo } = await import(
  `@/lib/sessions/access`
)

// EXP-1291/NEW-1: the ONE visibility rule — shared → members; else the
// starter. The device procedures belong to the device's account alone,
// derived from the devices rows (a hosted start sits on a shared device).

const session = (over: Record<string, unknown> = {}) =>
  ({ id: `s`, teamId: `t`, deviceId: `d`, shared: false, userId: `starter`, ...over }) as never

describe(`session access`, () => {
  it(`sees shared sessions as anyone, private ones as the starter only`, () => {
    expect(sessionVisibleTo(session({ shared: true }), `mate`)).toBe(true)
    expect(sessionVisibleTo(session(), `mate`)).toBe(false)
    expect(sessionVisibleTo(session(), `starter`)).toBe(true)
    expect(sessionVisibleTo(session(), `host`)).toBe(false)
  })

  it(`steering needs the rule AND current membership`, async () => {
    getTeamMember.mockResolvedValue({ role: `member` })
    await expect(assertCanSteer(`mate`, session({ shared: true }))).resolves.toBeUndefined()
    await expect(assertCanSteer(`mate`, session())).rejects.toMatchObject({ code: `FORBIDDEN` })
    getTeamMember.mockResolvedValue(undefined)
    await expect(assertCanSteer(`starter`, session())).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`the device procedures are the device account's (the derived host)`, async () => {
    resolveSessionHost.mockResolvedValue(`host`)
    await expect(sessionDeviceOwner(session())).resolves.toBe(`host`)
    await expect(assertDeviceHost(`starter`, session())).rejects.toThrow()
    await expect(assertDeviceHost(`host`, session())).resolves.toBeUndefined()
    resolveSessionHost.mockResolvedValue(null)
    await expect(sessionDeviceOwner(session())).resolves.toBe(`starter`)
    await expect(assertDeviceHost(`starter`, session())).resolves.toBeUndefined()
  })
})
