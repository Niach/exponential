import { randomBytes } from "crypto"

// One-time tickets for the LINK mode of the native browser handoff (EXP-1126).
//
// A signed-in native app cannot put its bearer token into a browser
// navigation, yet Better Auth's `/link-social` needs the session of the
// account being extended. So the app mints a ticket over tRPC
// (`users.mintSignInLinkTicket`) and opens
// `/api/mobile-oauth-start?link=<ticket>&provider=…`; the route redeems it
// for the SESSION ID it was minted under, looks the live session up and runs
// the link on the app's behalf. The ticket names one provider, lives two
// minutes, and burns on first use — an intercepted or replayed one is inert,
// and it never grants a browser session (Better Auth's link callback creates
// none). Same in-process store caveat as mobile-oauth-code.ts.

export const SIGN_IN_LINK_TICKET_TTL_MS = 2 * 60 * 1000

type Entry = {
  sessionId: string
  provider: string
  expiresAt: number
}

const store = ((globalThis as Record<string, unknown>).__expSignInLinkTickets ??=
  new Map<string, Entry>()) as Map<string, Entry>

function sweepExpired(now: number): void {
  for (const [ticket, entry] of store) {
    if (entry.expiresAt <= now) store.delete(ticket)
  }
}

// 32 random bytes, base64url: what mint produces and what redeem accepts.
export function isValidSignInLinkTicket(value: string): boolean {
  return /^[A-Za-z0-9_-]{43}$/.test(value)
}

export function mintSignInLinkTicket(
  input: { sessionId: string; provider: string },
  now = Date.now()
): string {
  sweepExpired(now)
  const ticket = randomBytes(32).toString(`base64url`)
  store.set(ticket, {
    sessionId: input.sessionId,
    provider: input.provider,
    expiresAt: now + SIGN_IN_LINK_TICKET_TTL_MS,
  })
  return ticket
}

// Consumed on ANY lookup hit (single-use, no oracle). Null when unknown or
// expired.
export function redeemSignInLinkTicket(
  ticket: string,
  now = Date.now()
): { sessionId: string; provider: string } | null {
  if (!isValidSignInLinkTicket(ticket)) return null
  const entry = store.get(ticket)
  if (!entry) return null
  store.delete(ticket)
  if (entry.expiresAt <= now) return null
  return { sessionId: entry.sessionId, provider: entry.provider }
}
