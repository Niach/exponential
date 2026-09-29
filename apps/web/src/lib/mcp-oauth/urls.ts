// URL rules of the MCP OAuth sign-in, dependency-free so the client, the
// credential store, the flows and the callback can all share them.
import { appBaseUrl } from "@/lib/notification-email-policy"

const PRIVATE_HOST =
  /^(localhost|.*\.localhost|.*\.local|.*\.internal|127\.\d+\.\d+\.\d+|10\.\d+\.\d+\.\d+|192\.168\.\d+\.\d+|172\.(1[6-9]|2\d|3[01])\.\d+\.\d+|169\.254\.\d+\.\d+|0\.0\.0\.0|\[::1\]|\[fc[0-9a-f]{2}:.*\]|\[fd[0-9a-f]{2}:.*\])$/i

/** Whether a provider on the public internet can FETCH our CIMD document:
 * https and a routable, dotted host. A LAN self-host or a dev box fails
 * this and registers a client dynamically instead (Sentry 500s on an
 * unreachable CIMD). */
export function isPublicHttpsBase(base: string): boolean {
  let url: URL
  try {
    url = new URL(base)
  } catch {
    return false
  }
  if (url.protocol !== `https:`) return false
  if (!url.hostname.includes(`.`) && !url.hostname.startsWith(`[`)) return false
  return !PRIVATE_HOST.test(url.hostname) && !PRIVATE_HOST.test(url.host)
}

/** The ONE redirect URI every sign-in uses: the hosted callback. */
export function hostedCallbackUrl(base = appBaseUrl()): string {
  return `${base.replace(/\/$/, ``)}/api/mcp-oauth/callback`
}

/** A same-origin RELATIVE path the callback may 302 to, else null: starts
 * with one `/` (not `//`, not `/\`), no scheme, no control chars or
 * backslashes, and its RESOLVED path does not start with `//` either (dot
 * segments — `/.//x`, `/a/..//x`, `/%2e//x` — collapse to a
 * protocol-relative `//x` a browser would treat as another host). Returns
 * the normalized path + query + hash. */
export function safeReturnTo(value: string | null | undefined): string | null {
  if (!value || value.length > 2048) return null
  if (!value.startsWith(`/`) || value.startsWith(`//`) || value.startsWith(`/\\`)) {
    return null
  }
  if (/[\u0000-\u001f\u007f\\]/.test(value)) return null
  try {
    const base = new URL(`https://exp.invalid`)
    const resolved = new URL(value, base)
    if (resolved.origin !== base.origin) return null
    if (!isSafePathname(resolved.pathname)) return null
    return `${resolved.pathname}${resolved.search}${resolved.hash}`
  } catch {
    return null
  }
}

function isSafePathname(pathname: string): boolean {
  return (
    pathname.startsWith(`/`) &&
    !pathname.startsWith(`//`) &&
    !pathname.includes(`\\`) &&
    !/^\/%2f/i.test(pathname) &&
    !/^\/%5c/i.test(pathname)
  )
}

/** `returnTo` plus the outcome params, preserving its own query/hash. The
 * output path always starts with exactly ONE `/` (leading slashes and
 * backslashes collapse), so it can never read as protocol-relative. */
export function returnUrl(
  returnTo: string,
  params: Record<string, string>
): string {
  const url = new URL(returnTo, `https://exp.invalid`)
  for (const [key, value] of Object.entries(params)) {
    url.searchParams.set(key, value)
  }
  const path = `/${url.pathname.replace(/^[\/\\]+/, ``)}`
  return `${path}${url.search}${url.hash}`
}
