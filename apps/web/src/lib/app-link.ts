/**
 * EXP-1188: what a tapped link in agent prose opens. Agents cite sources as
 * markdown links and link other runs/issues by their app URL; a link into
 * THIS instance routes in-app, anything else opens externally, and a link
 * with no real host (`https://…`) opens nothing. Hand-parsed so the rule is
 * byte-identical ×4: desktop `domain::app_link`, iOS `AppLink.swift`,
 * Android `AppLink.kt`, locked by `fixtures/app-link.json`.
 */

import { screenPath } from "@exp/domain-contract/apps"
import { crossAppTarget } from "@exponential_app/eui/host"
import { splitIssueRefs } from "@/lib/issue-refs"

export type AppLink =
  | { kind: `issue`; teamSlug: string; boardSlug: string; identifier: string }
  | { kind: `issueRef`; teamSlug: string; identifier: string }
  | { kind: `board`; teamSlug: string; boardSlug: string }
  | { kind: `session`; teamSlug: string; sessionId: string }
  | { kind: `plan`; teamSlug: string; sessionId: string; messageId: string }
  | { kind: `thread`; teamSlug: string; threadId: string }
  | { kind: `threads`; teamSlug: string }
  | { kind: `agent`; teamSlug: string; agentId: string }
  | { kind: `agentHome`; teamSlug: string }
  | { kind: `inbox`; teamSlug: string }
  | { kind: `devices`; teamSlug: string }
  | { kind: `settings`; teamSlug: string; section: string }
  | { kind: `appHome`; teamSlug: string; appSlug: string }
  | { kind: `appScreen`; teamSlug: string; appSlug: string; screen: string; params: Record<string, string> }
  | { kind: `app`; path: string }
  | { kind: `external`; url: string }
  | { kind: `ignore` }

const SCREEN_RESERVED: readonly string[] = [`tab`, `frame`]

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

/** VAPP-218: a path's query as screen params: `k=v` pairs split on `&`
 *  then the first `=`, each side percent-decoded (a bad escape kept), empty
 *  keys dropped, the last repeat wins, the editor's own keys dropped. */
function screenQueryParams(path: string): Record<string, string> {
  const q = path.indexOf(`?`)
  if (q === -1) return {}
  const h = path.indexOf(`#`, q)
  const query = path.slice(q + 1, h === -1 ? undefined : h)
  const found: Record<string, string> = {}
  for (const pair of query.split(`&`)) {
    const eq = pair.indexOf(`=`)
    const key = decodeSegment(eq === -1 ? pair : pair.slice(0, eq))
    if (!key || SCREEN_RESERVED.includes(key)) continue
    found[key] = eq === -1 ? `` : decodeSegment(pair.slice(eq + 1))
  }
  return Object.fromEntries(Object.keys(found).sort().map((k) => [k, found[k]!]))
}

/** An in-app path (query + hash kept for `app`, dropped for the routes). */
function classifyPath(path: string): AppLink {
  const end = path.search(/[?#]/)
  const segments = (end === -1 ? path : path.slice(0, end))
    .split(`/`)
    .filter((segment) => segment.length > 0)
    .map(decodeSegment)
  const [t, team, kind, a, b, c] = segments
  if (t === `t` && team && kind) {
    const n = segments.length
    const teamSlug = team
    if (kind === `issues` && a && n === 4) return { kind: `issueRef`, teamSlug, identifier: a }
    if (kind === `boards` && a && b === `issues` && c && n === 6)
      return { kind: `issue`, teamSlug, boardSlug: a, identifier: c }
    if (kind === `boards` && a && n === 4) return { kind: `board`, teamSlug, boardSlug: a }
    if (kind === `sessions` && a && b === `plan` && c && n === 6)
      return { kind: `plan`, teamSlug, sessionId: a, messageId: c }
    if (kind === `sessions` && a && n === 4) return { kind: `session`, teamSlug, sessionId: a }
    // EXP-1284: a thread (its `?child=` is dropped like every route query).
    if (kind === `threads` && a && n === 4) return { kind: `thread`, teamSlug, threadId: a }
    if (kind === `threads` && n === 3) return { kind: `threads`, teamSlug }
    if (kind === `agents` && a && n === 4) return { kind: `agent`, teamSlug, agentId: a }
    if (kind === `agent` && n === 3) return { kind: `agentHome`, teamSlug }
    if (kind === `inbox` && n === 3) return { kind: `inbox`, teamSlug }
    if (kind === `devices` && n === 3) return { kind: `devices`, teamSlug }
    if (kind === `settings` && a === `devices` && n === 4) return { kind: `devices`, teamSlug }
    if (kind === `settings` && a && n === 4) return { kind: `settings`, teamSlug, section: a }
    if (kind === `settings` && n === 3) return { kind: `settings`, teamSlug, section: `` }
    // VAPP-218: an app screen, its query = the screen's params.
    if (kind === `apps` && a && b && n === 5)
      return { kind: `appScreen`, teamSlug, appSlug: a, screen: b, params: screenQueryParams(path) }
    if (kind === `apps` && a && n === 4) return { kind: `appHome`, teamSlug, appSlug: a }
  }
  return { kind: `app`, path }
}

/** #EXP-1674: `~/x` = `/t/<team>/x`, or null when it cannot be expanded (no
 *  team, not `~/`, `~//`, a `.` or `..` segment even percent-encoded). */
export function expandTeamLink(href: string, team: string | null | undefined): string | null {
  if (!team || !href.startsWith(`~/`) || href.startsWith(`~//`)) return null
  const rest = href.slice(2)
  const end = rest.search(/[?#]/)
  const segments = (end === -1 ? rest : rest.slice(0, end)).split(`/`).map(decodeSegment)
  if (segments.some((segment) => segment === `.` || segment === `..`)) return null
  return `/t/${encodeURIComponent(team)}/${rest}`
}

export function classifyAppLink(rawHref: string, origin: string, team?: string | null): AppLink {
  const href = rawHref.trim()
  if (href.length === 0) return { kind: `ignore` }
  if (href.startsWith(`~`)) {
    const expanded = expandTeamLink(href, team)
    return expanded ? classifyPath(expanded) : { kind: `ignore` }
  }
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

/** #EXP-1674: what a chat card's link may open (plan-card.json `linkCases`):
 *  in-app, or https out. */
export function cardLinkOpens(href: string, origin: string, team?: string | null): boolean {
  const link = classifyAppLink(href, origin, team)
  if (link.kind === `ignore`) return false
  return link.kind !== `external` || link.url.toLowerCase().startsWith(`https://`)
}

/** #EXP-1674: a link an app's surface opens: any target but another app's
 *  screen as it is; another app's only through its `exposed` contract
 *  (fixtures host-router.json `crossApp`), with the params it declares. Null =
 *  the toast `LINK_NO_ACCESS`, the same for a missing app. A person's URL or an
 *  agent's chat link never comes here. */
export function crossAppLink(
  link: AppLink,
  sourceAppId: string,
  findApp: (teamSlug: string, appSlug: string) => { id: string; package: { screens?: unknown } | null } | null
): AppLink | null {
  if (link.kind !== `appHome` && link.kind !== `appScreen`) return link
  const target = findApp(link.teamSlug, link.appSlug)
  if (!target) return null
  if (target.id === sourceAppId) return link
  const decided = crossAppTarget(target.package ?? {}, link.kind === `appScreen` ? link.screen : null, link.kind === `appScreen` ? link.params : null)
  if (`error` in decided) return null
  return { kind: `appScreen`, teamSlug: link.teamSlug, appSlug: link.appSlug, screen: decided.screen, params: decided.params }
}

/** #EXP-1674: a Row subtitle's `#IDENT` refs that `known` resolves, as a host's
 *  `textLinks` segments on the team's short issue route; null = no link. */
export function issueTextLinks(
  text: string,
  team: string | null | undefined,
  known: (identifier: string) => boolean
): { text: string; href?: string }[] | null {
  if (!team || !text.includes(`#`)) return null
  let linked = false
  const segments = splitIssueRefs(text).map((segment) => {
    const href = segment.identifier && known(segment.identifier) ? expandTeamLink(`~/issues/${segment.identifier}`, team) : null
    if (!href) return { text: segment.text }
    linked = true
    return { text: segment.text, href }
  })
  return linked ? segments : null
}

/** The toast of a link the viewer cannot open (app-link.json `copy.noAccess`). */
export const LINK_NO_ACCESS = `Not found or no access`

export type LinkOpenOutcome = `open` | `switchTeam` | `noAccess` | `browser` | `notFound`

/** #EXP-1674: what opening a resolved in-app link does, the ONE rule ×4
 *  (app-link.json `openCases`). Unreadable looks exactly like missing. */
export function linkOpenOutcome(input: { member: boolean; activeTeam: boolean; found: boolean; webOnly: boolean }): LinkOpenOutcome {
  if (!input.member) return `noAccess`
  if (input.webOnly) return `browser`
  if (!input.found) return `notFound`
  return input.activeTeam ? `open` : `switchTeam`
}

/** #EXP-1674: markdown leaving the app (a PR body): every `](~/x)` link target
 *  made absolute. Code spans and fences stay as written, like any code. */
export function expandTeamLinksInMarkdown(markdown: string, team: string, origin: string): string {
  return markdown
    .split(/(```[\s\S]*?```|`[^`\n]*`)/)
    .map((part, i) =>
      i % 2 === 1
        ? part
        : part.replace(/\]\((~\/[^)\s]*)\)/g, (whole, href: string) => {
            const path = expandTeamLink(href, team)
            return path ? `](${origin}${path})` : whole
          })
    )
    .join(``)
}

// ── Web-only: the route of a classified in-app link ─────────────────────────

/** The team of a web path (`/t/<team>/…`), or null outside a team. */
export function teamSlugOfPath(pathname: string): string | null {
  const [t, team] = pathname.split(`/`).filter((segment) => segment.length > 0)
  return t === `t` && team ? decodeSegment(team) : null
}

/** The web path an in-app link navigates to (null: not an in-app link). */
export function appLinkPath(link: AppLink): string | null {
  const t = `teamSlug` in link ? `/t/${encodeURIComponent(link.teamSlug)}` : ``
  const e = encodeURIComponent
  switch (link.kind) {
    case `issue`:
      return `${t}/boards/${e(link.boardSlug)}/issues/${e(link.identifier)}`
    case `issueRef`:
      return `${t}/issues/${e(link.identifier)}`
    case `board`:
      return `${t}/boards/${e(link.boardSlug)}`
    case `session`:
      return `${t}/sessions/${e(link.sessionId)}`
    case `plan`:
      return `${t}/sessions/${e(link.sessionId)}/plan/${e(link.messageId)}`
    case `thread`:
      return `${t}/threads/${e(link.threadId)}`
    case `threads`:
      return `${t}/threads`
    case `agent`:
      return `${t}/agents/${e(link.agentId)}`
    case `agentHome`:
      return `${t}/agent`
    case `inbox`:
      return `${t}/inbox`
    case `devices`:
      return `${t}/devices`
    case `settings`:
      return link.section ? `${t}/settings/${e(link.section)}` : `${t}/settings`
    case `appHome`:
      return `${t}/apps/${e(link.appSlug)}`
    case `appScreen`:
      return screenPath(link.teamSlug, link.appSlug, link.screen, link.params)
    case `app`:
      return link.path
    case `external`:
    case `ignore`:
      return null
  }
}

// ── Web-only: which `app` paths the router actually serves ──────────────────
//
// `classifyAppLink` is the ×4 contract and calls EVERY same-origin path an
// `app` link. On the web an `app` link is pushed through the router, which
// turns a same-origin RESOURCE (`/api/attachments/<id>`, `/NOTICES.txt`,
// `/widget/...`) into the SPA's not-found page. Only paths under a screen
// prefix the route tree serves route in-app; everything else is left to the
// browser as an ordinary link. The list mirrors the top-level dirs of
// `routes/` minus the resource ones (`api`, `.well-known`, `robots.txt`);
// `app-link.test.ts` checks it against `routeTree.gen.ts`.

/** The first path segments the web router serves as screens. */
export const ROUTED_APP_PREFIXES: readonly string[] = [
  `about`,
  `admin`,
  `auth`,
  `exponential-ui-kitchen-sink`,
  `integrations`,
  `invite`,
  `onboarding`,
  `s`,
  `share`,
  `support`,
  `t`,
]

/** Whether an `app` link's path is one the router serves (the root
 *  included); false for a same-origin resource the browser must fetch. */
export function isRoutedAppPath(path: string): boolean {
  const end = path.search(/[?#]/)
  const pathname = end === -1 ? path : path.slice(0, end)
  const first = pathname.split(`/`).find((segment) => segment.length > 0)
  if (first === undefined) return true
  return ROUTED_APP_PREFIXES.includes(decodeSegment(first).toLowerCase())
}
