// eCode's remotes (fixtures/remote.json is the spec): parse a remote URL, the
// git config key that scopes a credential helper to exactly that remote (so
// a token never goes to another host), when to refresh a token, and the log
// redaction every runner and git call applies. ecode-core ports this file.

import { REDACTION, REFRESH_MARGIN_MS } from "./contract.generated"
import { byCodePoint } from "./forge"

export interface Remote {
  scheme: `https` | `exp`
  host: string
  path: string
}

const HOST = /^[A-Za-z0-9.-]+(:[0-9]{1,5})?$/
const SEGMENT = /^[A-Za-z0-9._-]+$/

/** `https://host[:port]/a/b[.git]` or `exp://<team>/<repo>`; null otherwise
 *  (http, ssh, credentials in the URL, a query or fragment, `..`). */
export function parseRemote(url: string): Remote | null {
  const match = /^(https|exp):\/\/([^/]+)\/(.+)$/.exec(url)
  if (!match) return null
  const scheme = match[1] as Remote[`scheme`]
  const host = match[2]!.toLowerCase()
  let path = match[3]!
  if (!HOST.test(host) || host.startsWith(`.`) || host.startsWith(`-`)) return null
  if (path.endsWith(`/`)) path = path.slice(0, -1)
  if (path.endsWith(`.git`)) path = path.slice(0, -4)
  const segments = path.split(`/`)
  if (segments.some((segment) => !SEGMENT.test(segment) || segment === `.` || segment === `..`)) return null
  if (scheme === `exp` && segments.length !== 1) return null
  return { scheme, host, path }
}

/** The canonical URL git is given (`https://host/path.git`, `exp://team/repo`). */
export function remoteUrl(remote: Remote): string {
  return remote.scheme === `https` ? `https://${remote.host}/${remote.path}.git` : `exp://${remote.host}/${remote.path}`
}

/** `credential.<url>.helper`, scoped to exactly this https remote; exp
 *  remotes authenticate inside `git-remote-exp`, so they get none. */
export function credentialHelperKey(url: string): string | null {
  const remote = parseRemote(url)
  if (!remote || remote.scheme !== `https`) return null
  return `credential.${remoteUrl(remote)}.helper`
}

/** Refresh when the token expires within the margin; null = never expires. */
export function needsRefresh(expiresAt: number | null, now: number, marginMs: number = REFRESH_MARGIN_MS): boolean {
  return expiresAt !== null && now + marginMs >= expiresAt
}

/** Every secret (longest first, empty ones ignored) replaced by the redaction mark. */
export function redact(text: string, secrets: readonly string[]): string {
  const sorted = [...new Set(secrets.filter((secret) => secret.length > 0))].sort(
    (a, b) => Array.from(b).length - Array.from(a).length || byCodePoint(a, b)
  )
  let out = text
  for (const secret of sorted) out = out.split(secret).join(REDACTION)
  return out
}
