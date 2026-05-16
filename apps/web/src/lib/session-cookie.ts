const SESSION_COOKIE_KEY = `session_token=`

// Better Auth emits the session token in one of potentially multiple
// Set-Cookie headers on its response; pull just that one value out so we can
// forward it to mobile clients as an opaque token.
export function parseSessionTokenFromSetCookie(
  headers: Headers
): string | null {
  const raw = headers.getSetCookie?.() ?? collectSetCookieFallback(headers)
  if (raw.length === 0) return null

  for (const entry of raw) {
    const segments = entry.split(`,`)
    for (const segment of segments) {
      const trimmed = segment.trim()
      if (!trimmed.startsWith(SESSION_COOKIE_KEY)) continue
      const valueWithAttrs = trimmed.slice(SESSION_COOKIE_KEY.length)
      const value = valueWithAttrs.split(`;`, 1)[0]
      if (!value) return null
      try {
        return decodeURIComponent(value)
      } catch {
        return value
      }
    }
  }

  return null
}

function collectSetCookieFallback(headers: Headers): string[] {
  const value = headers.get(`set-cookie`)
  return value ? [value] : []
}
