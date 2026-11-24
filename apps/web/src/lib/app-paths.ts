// #EXP-2027 (sidebar-tree.json): there is no active team on web md+. Home,
// Chats, Needs you, Drafts, the device pages and the personal settings are
// TEAM-LESS pages; what belongs to a team (boards, issues, sessions, agents,
// apps, team settings, the creation pages) keeps `/t/$teamSlug/…`. Phones
// (#EXP-2109, web-mobile) have no active team either: an old team-scoped URL
// forwards to its team-less page on both.
// Pure, so every mapping is a test.

/** Personal settings pages (settings-nav.json `desktopNavs.personal`; About
 *  lives in the account menu on web). */
export const PERSONAL_SETTINGS_PAGES = [`account`, `assistant`, `notifications`, `security`] as const
export type PersonalSettingsPage = (typeof PERSONAL_SETTINGS_PAGES)[number]

/** The creation pages a team menu's New › X opens (sidebar-tree.json). */
export const NEW_PAGE_KINDS = [`board`, `agent`, `app`] as const
export type NewPageKind = (typeof NEW_PAGE_KINDS)[number]

const TEAM_REST = /^\/t\/([^/]+)(\/.*)?$/

const trim = (path: string) => (path.length > 1 ? path.replace(/\/+$/, ``) : path)

/** md+: the team-less page an old team-scoped path moved to; null = it stays. */
export function widePathFor(pathname: string): string | null {
  const match = TEAM_REST.exec(trim(pathname))
  if (!match) return trim(pathname) === `/` ? `/home` : null
  const slug = match[1]
  const rest = match[2] ?? ``
  switch (rest) {
    case `/agent`:
      return `/home`
    case `/chats`:
      return `/chats`
    case `/needs-you`:
      return `/needs-you`
    case `/drafts`:
      return `/drafts`
    // The Devices page is gone: each device is a sidebar row.
    case `/devices`:
    case `/team`:
      return `/home`
    case `/settings/voice`:
      return `/settings/assistant`
    case `/settings/agents/new`:
      return `/t/${slug}/new/agent`
  }
  const settings = /^\/settings\/([^/]+)$/.exec(rest)
  if (settings && (PERSONAL_SETTINGS_PAGES as readonly string[]).includes(settings[1])) {
    return `/settings/${settings[1]}`
  }
  return null
}

/** Phones (#EXP-2109): the same, plus the pages only phones have: the Devices
 *  list, and the team's Teams page for its old Apps tab. */
export function phonePathFor(pathname: string): string | null {
  const path = trim(pathname)
  if (path === `/`) return `/chats`
  const match = TEAM_REST.exec(path)
  if (match) {
    const rest = match[2] ?? ``
    if (rest === `` || rest === `/apps`) return `/t/${match[1]}/team`
    if (rest === `/devices`) return `/devices`
    if (rest === `/inbox`) return `/inbox`
    if (rest === `/team` || rest === `/settings/agents/new`) return null
  }
  return widePathFor(path)
}

/** The team-less pages' paths. */
export const homePath = () => `/home`
export const devicePath = (deviceId: string) => `/devices/${encodeURIComponent(deviceId)}`
export const deviceSettingsPath = (deviceId: string) => `${devicePath(deviceId)}/settings`
export const personalSettingsPath = (page: PersonalSettingsPage = `account`) => `/settings/${page}`
export const newPagePath = (teamSlug: string, kind: NewPageKind) => `/t/${encodeURIComponent(teamSlug)}/new/${kind}`

/** A team-less settings path (`/settings`, `/settings/<page>`). */
export function isPersonalSettingsPath(pathname: string): boolean {
  const path = trim(pathname)
  return path === `/settings` || path.startsWith(`/settings/`)
}

/** The team settings of `/t/<slug>/settings/…`: its slug, else null. */
export function teamSettingsSlug(pathname: string): string | null {
  const match = TEAM_REST.exec(trim(pathname))
  if (!match) return null
  const rest = match[2] ?? ``
  return rest === `/settings` || rest.startsWith(`/settings/`) ? match[1] : null
}
