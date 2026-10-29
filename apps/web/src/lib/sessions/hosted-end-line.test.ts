import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

const { hostedEndLine } = await import(`@/lib/sessions/lifecycle`)

// FEED-154/FEED-159: the end line a hosted run reads when its share went.
// An instance admin's key reset names no one (the admin is no teammate).
describe(`hostedEndLine`, () => {
  it(`names the actor, except for an instance admin's key reset`, () => {
    expect(hostedEndLine(`Ada`, { deviceLabel: `box`, cause: `unshared` })).toBe(
      `Ended: Ada stopped sharing box with this team.`
    )
    expect(hostedEndLine(`Ada`, { deviceLabel: `box`, cause: `dropped` })).toBe(
      `Ended: Ada removed box's share with this team.`
    )
    expect(hostedEndLine(`Ada`, { deviceLabel: `box`, cause: `admin` })).toBe(
      `Ended: An instance admin reset box's key, which removed its share with this team.`
    )
  })
})
