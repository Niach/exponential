// #PERF-7: the team layout's gate passes a confirmed, still-synced team
// without a round trip; everything else keeps the server check.
import { beforeEach, describe, expect, it, vi } from "vitest"

const auth = vi.hoisted(() => ({
  cached: null as null | { session: { id: string }; user: { id: string } },
  fresh: null as null | { session: { id: string }; user: { id: string } },
}))
vi.mock(`@/lib/auth/client`, () => ({
  peekSession: () => auth.cached,
  fetchSessionOnce: async () => auth.fresh,
}))

const { fastTeamGate, forgetTeamMemberships, rememberTeamMembership } = await import(`./team-route-gate`)

const alice = { session: { id: `s1` }, user: { id: `alice` } }
const synced = () => true
const notSynced = () => false

describe(`fastTeamGate`, () => {
  beforeEach(() => {
    forgetTeamMemberships()
    auth.cached = alice
    auth.fresh = alice
  })

  it(`asks the server on a first visit`, () => {
    expect(fastTeamGate(`acme`, synced, vi.fn())).toBeNull()
  })

  it(`passes a confirmed, synced team without waiting`, () => {
    rememberTeamMembership(`alice`, `acme`)
    expect(fastTeamGate(`acme`, synced, vi.fn())).toBe(alice)
  })

  it(`asks the server again once the team left the members-only shape`, () => {
    rememberTeamMembership(`alice`, `acme`)
    expect(fastTeamGate(`acme`, notSynced, vi.fn())).toBeNull()
  })

  it(`never carries a confirmation over to another user or team`, () => {
    rememberTeamMembership(`bob`, `acme`)
    rememberTeamMembership(`alice`, `globex`)
    expect(fastTeamGate(`acme`, synced, vi.fn())).toBeNull()
  })

  it(`asks the server without a cached session`, () => {
    rememberTeamMembership(`alice`, `acme`)
    auth.cached = null
    expect(fastTeamGate(`acme`, synced, vi.fn())).toBeNull()
  })

  it(`sends a user the background refresh finds signed out to the login`, async () => {
    rememberTeamMembership(`alice`, `acme`)
    auth.fresh = null
    const onSignedOut = vi.fn()
    expect(fastTeamGate(`acme`, synced, onSignedOut)).toBe(alice)
    await Promise.resolve()
    await Promise.resolve()
    expect(onSignedOut).toHaveBeenCalledTimes(1)
  })

  it(`leaves a signed-in user alone after the refresh`, async () => {
    rememberTeamMembership(`alice`, `acme`)
    const onSignedOut = vi.fn()
    fastTeamGate(`acme`, synced, onSignedOut)
    await Promise.resolve()
    await Promise.resolve()
    expect(onSignedOut).not.toHaveBeenCalled()
  })
})
