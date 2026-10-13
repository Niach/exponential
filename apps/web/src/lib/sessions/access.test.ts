import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))
const getTeamMember = vi.fn()
vi.mock(`@/lib/auth/membership`, () => ({ getTeamMember: (...args: unknown[]) => getTeamMember(...args) }))

const { assertCanSteer, assertDeviceHost, sessionDeviceOwner, sessionVisibleTo } = await import(
  `@/lib/sessions/access`
)

// EXP-1291: the ONE visibility rule — shared → members; else the starter
// (and the host running it). The device procedures belong to the device's
// account alone.

const session = (over: Record<string, unknown> = {}) =>
  ({ id: `s`, teamId: `t`, shared: false, userId: `starter`, hostUserId: null, ...over }) as never

describe(`session access`, () => {
  it(`sees shared sessions as anyone, private ones as starter or host`, () => {
    expect(sessionVisibleTo(session({ shared: true }), `mate`)).toBe(true)
    expect(sessionVisibleTo(session(), `mate`)).toBe(false)
    expect(sessionVisibleTo(session(), `starter`)).toBe(true)
    expect(sessionVisibleTo(session({ hostUserId: `host` }), `host`)).toBe(true)
  })

  it(`steering needs the rule AND current membership`, async () => {
    getTeamMember.mockResolvedValue({ role: `member` })
    await expect(assertCanSteer(`mate`, session({ shared: true }))).resolves.toBeUndefined()
    await expect(assertCanSteer(`mate`, session())).rejects.toMatchObject({ code: `FORBIDDEN` })
    getTeamMember.mockResolvedValue(undefined)
    await expect(assertCanSteer(`starter`, session())).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`the device procedures are the device account's`, () => {
    expect(sessionDeviceOwner(session({ hostUserId: `host` }))).toBe(`host`)
    expect(() => assertDeviceHost(`starter`, session({ hostUserId: `host` }))).toThrow()
    expect(() => assertDeviceHost(`host`, session({ hostUserId: `host` }))).not.toThrow()
    expect(() => assertDeviceHost(`starter`, session())).not.toThrow()
  })
})
