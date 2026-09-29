// EXP-1132: the invite a signed-out visitor opened, remembered across the
// sign-in detour. `?redirect=/invite/<token>` rides the OAuth callbackURL, but
// any hop that drops it (a provider error, a second trip through login, an
// in-app mail browser) used to land the new account on the create-or-join
// wizard with the invite forgotten. The invite page writes it, onboarding
// resumes it, and a successful (or dead) invite clears it.
const STORAGE_KEY = `exp.pendingInvite`
const TTL_MS = 24 * 60 * 60 * 1000

interface StoredInvite {
  token: string
  savedAt: number
}

/** Pure: the stored token when the payload is well-formed and fresh. */
export function parsePendingInvite(
  raw: string | null,
  now: number = Date.now()
): string | null {
  if (!raw) return null
  try {
    const value = JSON.parse(raw) as Partial<StoredInvite>
    if (typeof value.token !== `string` || value.token.length === 0) return null
    if (typeof value.savedAt !== `number`) return null
    if (now - value.savedAt > TTL_MS || value.savedAt > now + TTL_MS) return null
    return value.token
  } catch {
    return null
  }
}

function storage(): Storage | null {
  try {
    return typeof window === `undefined` ? null : window.localStorage
  } catch {
    return null
  }
}

export function rememberPendingInvite(token: string): void {
  try {
    storage()?.setItem(
      STORAGE_KEY,
      JSON.stringify({ token, savedAt: Date.now() } satisfies StoredInvite)
    )
  } catch {
    // Storage full or blocked: the redirect param still carries the invite.
  }
}

export function readPendingInvite(): string | null {
  try {
    return parsePendingInvite(storage()?.getItem(STORAGE_KEY) ?? null)
  } catch {
    return null
  }
}

export function clearPendingInvite(): void {
  try {
    storage()?.removeItem(STORAGE_KEY)
  } catch {
    // Nothing to do — a stale entry expires on its own.
  }
}

/** Clear only when the remembered invite IS this one (another tab's stays). */
export function clearPendingInviteFor(token: string): void {
  if (readPendingInvite() === token) clearPendingInvite()
}

/**
 * Pure: where a completed login goes next. An explicit destination (the
 * `redirect` param or the MCP OAuth resume) always wins; without one the
 * remembered invite is resumed, so an EXISTING member whose OAuth return lost
 * the redirect (a state-cookie drop lands on `/auth/login?error=...`, no
 * `redirect`) still reaches the invite instead of their old team while the
 * invite idles in storage for a day.
 */
export function resolveLoginDestination(
  destination: string | null | undefined,
  pendingInvite: string | null | undefined
): string {
  if (destination) return destination
  if (pendingInvite) return `/invite/${pendingInvite}`
  return `/`
}

/**
 * Pure: the login page's `error` search value. The native OAuth hop bounces
 * failures here as `?error=<reason>` (REV2-53); Better Auth itself sends a
 * callback that carries NO `state` query to `/auth/login?state=state_not_found`
 * (a `state` param, not `error`), which used to render a blank form. Fold that
 * marker into the same error slug the copy map explains.
 */
export const STATE_NOT_FOUND = `state_not_found`

export function loginErrorFromSearch(search: Record<string, unknown>): string | undefined {
  if (typeof search.error === `string` && search.error.length > 0) return search.error
  if (search.state === STATE_NOT_FOUND) return STATE_NOT_FOUND
  return undefined
}
