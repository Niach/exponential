/**
 * The `?redirect` search param on /auth/login and /auth/register is
 * attacker-controllable (anyone can craft a link) and its value ends up in
 * `window.location.href` after sign-in. Allow only same-origin absolute
 * paths:
 *  - must be a string starting with `/` — anything else can carry a scheme
 *    (`javascript:alert(1)`, `https://evil.example`)
 *  - `//host` (protocol-relative) and `/\host` (browsers normalize `\` to
 *    `/` in http(s) URLs) are cross-origin escapes — rejected
 *  - ASCII control chars are rejected outright: HTML URL parsing strips
 *    tab/newline BEFORE parsing, so `/\t/evil.example` would otherwise
 *    become `//evil.example`
 * Returns undefined for anything unsafe; callers fall back to `/`.
 */
export function sanitizeRedirectPath(value: unknown): string | undefined {
  if (typeof value !== `string` || value.length === 0) return undefined
  for (let i = 0; i < value.length; i++) {
    if (value.charCodeAt(i) < 0x20) return undefined
  }
  if (!value.startsWith(`/`)) return undefined
  if (value.startsWith(`//`) || value.startsWith(`/\\`)) return undefined
  return value
}
