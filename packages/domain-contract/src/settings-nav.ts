// #EXP-1381 (owner 2026-10-24): the phones' Team-page settings rows, each
// opening its OWN page. Locked by `fixtures/settings-nav.json` (its
// `$comment` holds the rules); mirrors: iOS ExpCore `SettingsNav` and Android
// `domain.SettingsNav`, both generated from the fixture.

import fixture from "../fixtures/settings-nav.json" with { type: "json" }

export type SettingsNavVisibility = `all` | `owner` | `ownerCloud`
export type SettingsNavCount = `members` | `boards` | `agents`

export interface SettingsNavRow {
  id: string
  label: string
  icon: string
  group: string
  visible: SettingsNavVisibility
  phone: boolean
  count?: SettingsNavCount
}

export const SETTINGS_NAV_ROWS = fixture.rows as SettingsNavRow[]
export const SETTINGS_NAV_PHONE_ROWS = fixture.phoneRows
export const SETTINGS_NAV_TEST_IDS = fixture.testIds

export function settingsNavRow(id: string): SettingsNavRow | undefined {
  return SETTINGS_NAV_ROWS.find((row) => row.id === id)
}

/** `visible`, the same on every client (`visibleCases`): `isOwner` =
 *  teamRole owner, `cloud` = a cloud instance. */
export function settingsNavVisible(
  row: SettingsNavRow,
  viewer: { isOwner: boolean; isCloud: boolean }
): boolean {
  switch (row.visible) {
    case `all`:
      return true
    case `owner`:
      return viewer.isOwner
    case `ownerCloud`:
      return viewer.isOwner && viewer.isCloud
  }
}

/** The phone rows a viewer sees, in order. */
export function settingsNavPhoneRows(viewer: { isOwner: boolean; isCloud: boolean }): string[] {
  return SETTINGS_NAV_PHONE_ROWS.filter((id) => {
    const row = settingsNavRow(id)
    return row !== undefined && settingsNavVisible(row, viewer)
  })
}

// S-3 (owner 2026-10-28): THE settings nav ×4 = `nav` (groups, injected
// `@lists`, `children`, `pages`, per-platform pages). `navCases` lock it.
export type SettingsNavPlatform = `web` | `web-mobile` | `desktop` | `ios` | `android`

export interface SettingsNavPage {
  label: string
  icon: string
  visible: SettingsNavVisibility
  platforms?: SettingsNavPlatform[]
}

export interface SettingsNavGroup {
  id: string
  label: string
  platforms?: SettingsNavPlatform[]
  items: string[]
}

export const SETTINGS_NAV_GROUPS = fixture.nav.groups as SettingsNavGroup[]
export const SETTINGS_NAV_CHILDREN = fixture.nav.children as Record<string, string[]>
export const SETTINGS_NAV_PAGES = fixture.nav.pages as Record<string, SettingsNavPage>
const ROW_PLATFORMS = fixture.nav.rowPlatforms as Record<string, SettingsNavPlatform[]>
const NEW_ENTRIES = fixture.nav.newEntries as Record<string, SettingsNavVisibility>

/** The label + icon of a page id (a `rows` row or a `pages` page). */
export function settingsNavPage(id: string): SettingsNavPage | undefined {
  const row = settingsNavRow(id)
  if (row) return { label: row.label, icon: row.icon, visible: row.visible, platforms: ROW_PLATFORMS[id] }
  return SETTINGS_NAV_PAGES[id]
}

/** One item (page id or `@list`) for a viewer on a platform. */
export function settingsNavItemVisible(
  item: string,
  platform: SettingsNavPlatform,
  viewer: { isOwner: boolean; isCloud: boolean }
): boolean {
  if (item.startsWith(`@`)) {
    const visible = NEW_ENTRIES[item]
    return visible === undefined || settingsNavVisible({ visible } as SettingsNavRow, viewer)
  }
  const page = settingsNavPage(item)
  if (!page) return false
  if (page.platforms && !page.platforms.includes(platform)) return false
  return settingsNavVisible({ visible: page.visible } as SettingsNavRow, viewer)
}

/** The nav's groups for a viewer on a platform: group id → items in order
 *  (an empty group is dropped). */
export function settingsNavGroups(
  platform: SettingsNavPlatform,
  viewer: { isOwner: boolean; isCloud: boolean }
): Record<string, string[]> {
  const out: Record<string, string[]> = {}
  for (const group of SETTINGS_NAV_GROUPS) {
    if (group.platforms && !group.platforms.includes(platform)) continue
    const items = group.items.filter((item) => settingsNavItemVisible(item, platform, viewer))
    if (items.length > 0) out[group.id] = items
  }
  return out
}

// #EXP-2027 (owner 2026-11-20): THE SPLIT on web md+ and the desktop IDE.
// `desktopNavs` = the personal nav (the gear by the member's name), a team's
// nav (its row menu › Settings, `issues` keeps its `children` folded) and the
// This device pages (the device page's, desktop only). `desktopNavCases` lock it.
export interface SettingsDesktopNavGroup {
  id: string
  label: string
  items: string[]
}

export interface SettingsDesktopNavs {
  personal: string[]
  team: string[]
  device: string[]
}

const DESKTOP_NAVS = fixture.nav.desktopNavs as {
  personal: SettingsDesktopNavGroup[]
  team: SettingsDesktopNavGroup[]
  device: string[]
}

export const SETTINGS_DESKTOP_NAV_GROUPS = {
  personal: DESKTOP_NAVS.personal,
  team: DESKTOP_NAVS.team,
}
export const SETTINGS_DESKTOP_DEVICE_PAGES = DESKTOP_NAVS.device

/** The visible page ids of each desktop nav for a viewer on a platform. */
export function settingsDesktopNavs(viewer: {
  platform: SettingsNavPlatform
  isOwner: boolean
  isCloud: boolean
}): SettingsDesktopNavs {
  const visible = (id: string) => settingsNavItemVisible(id, viewer.platform, viewer)
  return {
    personal: DESKTOP_NAVS.personal.flatMap((g) => g.items).filter(visible),
    team: DESKTOP_NAVS.team.flatMap((g) => g.items).filter(visible),
    device: DESKTOP_NAVS.device.filter(visible),
  }
}
