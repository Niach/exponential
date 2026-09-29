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
