import { describe, expect, it } from "vitest"
import {
  SIGN_IN_LINK_TICKET_TTL_MS,
  isValidSignInLinkTicket,
  mintSignInLinkTicket,
  redeemSignInLinkTicket,
} from "@/lib/auth/sign-in-link-ticket"

describe(`sign-in link tickets`, () => {
  it(`round-trips the session id and provider`, () => {
    const ticket = mintSignInLinkTicket({ sessionId: `s1`, provider: `google` })
    expect(isValidSignInLinkTicket(ticket)).toBe(true)
    expect(redeemSignInLinkTicket(ticket)).toEqual({
      sessionId: `s1`,
      provider: `google`,
    })
  })

  it(`is single-use`, () => {
    const ticket = mintSignInLinkTicket({ sessionId: `s2`, provider: `apple` })
    expect(redeemSignInLinkTicket(ticket)).not.toBeNull()
    expect(redeemSignInLinkTicket(ticket)).toBeNull()
  })

  it(`expires after the TTL and redeems just inside it`, () => {
    const t = 1_700_000_000_000
    const late = mintSignInLinkTicket({ sessionId: `s3`, provider: `okta` }, t)
    expect(redeemSignInLinkTicket(late, t + SIGN_IN_LINK_TICKET_TTL_MS + 1)).toBeNull()
    const inTime = mintSignInLinkTicket({ sessionId: `s4`, provider: `okta` }, t)
    expect(
      redeemSignInLinkTicket(inTime, t + SIGN_IN_LINK_TICKET_TTL_MS - 1)
    ).toEqual({ sessionId: `s4`, provider: `okta` })
  })

  it(`rejects unknown and malformed tickets`, () => {
    expect(redeemSignInLinkTicket(`no-such-ticket`)).toBeNull()
    expect(isValidSignInLinkTicket(`short`)).toBe(false)
    expect(isValidSignInLinkTicket(`${`a`.repeat(42)}+`)).toBe(false)
  })

  it(`mints distinct tickets per call`, () => {
    const a = mintSignInLinkTicket({ sessionId: `s`, provider: `google` })
    const b = mintSignInLinkTicket({ sessionId: `s`, provider: `google` })
    expect(a).not.toBe(b)
  })
})
