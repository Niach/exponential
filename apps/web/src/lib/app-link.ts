/**
 * EXP-1188: what a tapped link in agent prose opens. Agents cite sources as
 * markdown links and link other runs/issues by their app URL; a link into
 * THIS instance routes in-app, anything else opens externally, and a link
 * with no real host (`https://…`) opens nothing. Hand-parsed so the rule is
 * byte-identical ×4: desktop `domain::app_link`, iOS `AppLink.swift`,
 * Android `AppLink.kt`, locked by `fixtures/app-link.json`.
 */

export type AppLink =
  | { kind: `issue`; teamSlug: string; boardSlug: string; identifier: string }
  | { kind: `session`; teamSlug: string; sessionId: string }
  | { kind: `app`; path: string }
  | { kind: `external`; url: string }
  | { kind: `ignore` }

// Two labels at least (or `localhost`): a browser punycodes the placeholder
// `https://…` into the one-label host `xn--rvg`.
const AUTHORITY = /^([A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+|localhost)(:[0-9]+)?$/i

function decodeSegment(segment: string): string {
  try {
    return decodeURIComponent(segment)
  } catch {
    return segment
  }
}

/** An in-app path (query + hash kept for `app`, dropped for the routes). */
function classifyPath(path: string): AppLink {
  const end = path.search(/[?#]/)
  const segments = (end === -1 ? path : path.slice(0, end))
    .split(`/`)
    .filter((segment) => segment.length > 0)
    .map(decodeSegment)
  const [t, team, kind, a, b, c] = segments
  if (t === `t` && team) {
    if (kind === `boards` && a && b === `issues` && c && segments.length === 6)
      return { kind: `issue`, teamSlug: team, boardSlug: a, identifier: c }
    if (kind === `sessions` && a && segments.length === 4)
      return { kind: `session`, teamSlug: team, sessionId: a }
  }
  return { kind: `app`, path }
}

export function classifyAppLink(rawHref: string, origin: string): AppLink {
  const href = rawHref.trim()
  if (href.length === 0) return { kind: `ignore` }
  if (href.startsWith(`/`))
    return href.startsWith(`//`) ? { kind: `ignore` } : classifyPath(href)
  const lower = href.toLowerCase()
  if (lower.startsWith(`mailto:`)) return { kind: `external`, url: href }
  const scheme = lower.startsWith(`https://`)
    ? `https://`
    : lower.startsWith(`http://`)
      ? `http://`
      : null
  if (!scheme) return { kind: `ignore` }
  const rest = href.slice(scheme.length)
  const end = rest.search(/[/?#]/)
  const authority = end === -1 ? rest : rest.slice(0, end)
  if (!AUTHORITY.test(authority)) return { kind: `ignore` }
  if (`${scheme}${authority}`.toLowerCase() !== origin.toLowerCase())
    return { kind: `external`, url: href }
  const path = end === -1 ? `/` : rest.slice(end)
  return classifyPath(path.startsWith(`/`) ? path : `/${path}`)
}
