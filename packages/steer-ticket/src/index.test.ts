import { describe, expect, test } from "bun:test"
import {
  signSteerTicket,
  verifySteerTicket,
  type SteerTicketClaims,
} from "./index"

const SECRET = `test-secret-0123456789abcdef`

function claims(overrides: Partial<SteerTicketClaims> = {}): SteerTicketClaims {
  const now = Math.floor(Date.now() / 1000)
  return {
    sub: `user-1`,
    team: `team-1`,
    sessionId: `sess-1`,
    role: `viewer`,
    iat: now,
    exp: now + 60,
    ...overrides,
  }
}

describe(`steer tickets`, () => {
  test(`round-trips claims`, () => {
    const c = claims()
    const token = signSteerTicket(c, SECRET)
    const result = verifySteerTicket(token, SECRET)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.claims).toEqual(c)
  })

  test(`rejects a tampered payload`, () => {
    const token = signSteerTicket(claims(), SECRET)
    const [payload, sig] = token.split(`.`)
    const forged = Buffer.from(
      JSON.stringify({ ...claims(), sub: `attacker` }),
      `utf8`
    ).toString(`base64url`)
    const result = verifySteerTicket(`${forged}.${sig}`, SECRET)
    expect(result).toEqual({ ok: false, reason: `bad_signature` })
    expect(payload).not.toBe(forged)
  })

  test(`rejects the wrong secret`, () => {
    const token = signSteerTicket(claims(), SECRET)
    const result = verifySteerTicket(token, `another-secret`)
    expect(result).toEqual({ ok: false, reason: `bad_signature` })
  })

  test(`rejects expired tickets`, () => {
    const now = Math.floor(Date.now() / 1000)
    const token = signSteerTicket(claims({ exp: now - 1 }), SECRET)
    const result = verifySteerTicket(token, SECRET)
    expect(result).toEqual({ ok: false, reason: `expired` })
  })

  // EXP-710: `deviceLabel` was dropped from the claims, but a control ticket
  // minted seconds before the deploy still carries it — verification must
  // stay tolerant of legacy claims (and of any future one), never schema-check
  // them, or an in-flight desktop is locked out for a minute.
  test(`verifies a ticket carrying a legacy claim`, () => {
    const legacy = {
      ...claims({ role: `control`, team: `` }),
      deviceLabel: `My MacBook`,
    } as SteerTicketClaims
    const result = verifySteerTicket(signSteerTicket(legacy, SECRET), SECRET)
    expect(result.ok).toBe(true)
    if (result.ok) {
      expect(result.claims.role).toBe(`control`)
      expect(result.claims.sub).toBe(`user-1`)
      // Unknown claims ride through untouched — nothing reads them.
      expect((result.claims as Record<string, unknown>).deviceLabel).toBe(
        `My MacBook`
      )
    }
  })

  // NEW-55: the call roles ride the same format — callId/agentId are plain
  // claims, verified (and tolerated) like every other one.
  test(`round-trips caller and call_host tickets`, () => {
    const caller = claims({
      role: `caller`,
      team: ``,
      sessionId: undefined,
      callId: `call-1`,
      agentId: `agent-1`,
      deviceId: `dev-1`,
      deviceOwnerId: `user-1`,
    })
    const host = claims({ role: `call_host`, team: ``, sessionId: undefined, callId: `call-1` })
    for (const c of [caller, host]) {
      const result = verifySteerTicket(signSteerTicket(c, SECRET), SECRET)
      expect(result.ok).toBe(true)
      if (result.ok) {
        expect(result.claims.role).toBe(c.role)
        expect(result.claims.callId).toBe(`call-1`)
        expect(result.claims.agentId).toBe(c.agentId)
      }
    }
  })

  test(`rejects malformed tokens`, () => {
    for (const bad of [``, `abc`, `.`, `a.`, `.b`, `not-base64!.sig`]) {
      const result = verifySteerTicket(bad, SECRET)
      expect(result.ok).toBe(false)
    }
  })
})
