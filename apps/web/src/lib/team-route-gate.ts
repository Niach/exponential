import { fetchSessionOnce, peekSession, type SessionData } from "@/lib/auth/client"

// #PERF-7: the `/t/$teamSlug` layout's beforeLoad ran on EVERY navigation
// inside a team and awaited, in series, the auth session (past its 30 s
// cache) and `teams.getBySlug` before the router even started the next
// page: two round trips in front of each session, settings or needs-you
// page. Once the server confirmed this user's membership of a slug in this
// page life, and the members-only `teams` shape still holds that team (a
// removal deletes the row live, sending the next navigation back through
// the server check), the gate passes at once and refreshes the session in
// the background. A first visit, another user, or a team that left the
// shape keeps the full server check.

/** How old a cached session may be for a confirmed team's fast path. */
export const STALE_SESSION_MS = 10 * 60_000

const confirmed = new Set<string>()
const pairKey = (userId: string, slug: string) => `${userId}\u0000${slug}`

/** The server answered `teams.getBySlug` for this user and slug. */
export function rememberTeamMembership(userId: string, slug: string): void {
  confirmed.add(pairKey(userId, slug))
}

/** Test seam: forget every confirmation. */
export function forgetTeamMemberships(): void {
  confirmed.clear()
}

/**
 * The session to pass through without a round trip, or null when the gate
 * must ask the server. `teamSynced` = the synced `teams` shape holds `slug`.
 * On a hit the session refreshes in the background; a refresh that finds
 * the user signed out calls `onSignedOut` (the login page).
 */
export function fastTeamGate(
  slug: string,
  teamSynced: (slug: string) => boolean,
  onSignedOut: () => void
): SessionData | null {
  const cached = peekSession(STALE_SESSION_MS)
  if (!cached?.user?.id || !confirmed.has(pairKey(cached.user.id, slug))) return null
  if (!teamSynced(slug)) return null
  void fetchSessionOnce().then((fresh) => {
    if (!fresh) onSignedOut()
  })
  return cached
}
