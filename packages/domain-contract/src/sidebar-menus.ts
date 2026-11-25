// #EXP-2124: the sidebar menus, the pure reference of
// `fixtures/sidebar-menus.json` (its `$comment` is the spec). ONE menu table
// for every sidebar row, THE New list and the new-tab menu; web replays it
// here, desktop in Rust `domain::sidebar_menus`.

import fixture from "../fixtures/sidebar-menus.json" with { type: "json" }

export type SidebarMenuGroup = `open` | `kind` | `arrange` | `settings` | `danger`
export type SidebarMenuKind = keyof typeof fixture.kinds
export type SidebarMenuAction = keyof typeof fixture.icons
export type SidebarMenuEntryId = SidebarMenuAction | `separator`
export type SidebarMenuFlag = `owner` | `mine` | `canDelete`
export type SidebarNewKind = `session` | `agent` | `app` | `board`

/** What decides a kind's conditional rows. */
export type SidebarMenuInput = Partial<Record<SidebarMenuFlag, boolean>>

type TableEntry = SidebarMenuAction | { id: SidebarMenuAction; when: SidebarMenuFlag }

export const SIDEBAR_MENU_GROUPS = fixture.groups as readonly SidebarMenuGroup[]
export const SIDEBAR_MENU_KINDS = Object.keys(fixture.kinds) as SidebarMenuKind[]
export const SIDEBAR_MENU_COPY = fixture.copy
export const SIDEBAR_MENU_ICONS = fixture.icons
export const SIDEBAR_MENU_SHORTCUTS = fixture.shortcuts as Partial<
  Record<SidebarMenuAction, { apple: string; other: string }>
>
export const SIDEBAR_NEW_KINDS = fixture.newKinds as readonly SidebarNewKind[]
export const SIDEBAR_NEW_COPY = fixture.newCopy as Record<SidebarNewKind, string>
export const SIDEBAR_NEW_ICONS = fixture.newIcons as Record<SidebarNewKind, string>
/** The sidebar kinds whose page opens as a work tab (`open-new-tab`). */
export const SIDEBAR_TAB_PAGES = fixture.tabs.pages as readonly SidebarMenuKind[]

/** A `danger` row: drawn destructive, asks first. */
export function sidebarMenuDestructive(kind: SidebarMenuKind, action: SidebarMenuAction): boolean {
  return (fixture.kinds[kind].danger as TableEntry[]).some((entry) => (typeof entry === `string` ? entry : entry.id) === action)
}

/** A kind's menu for the viewer: the groups in order, `separator` between
 *  the non-empty ones. */
export function sidebarItemMenu(kind: SidebarMenuKind, input: SidebarMenuInput = {}): SidebarMenuEntryId[] {
  const table = fixture.kinds[kind] as Record<SidebarMenuGroup, TableEntry[]>
  const out: SidebarMenuEntryId[] = []
  for (const group of SIDEBAR_MENU_GROUPS) {
    const rows = table[group].flatMap((entry) =>
      typeof entry === `string` ? [entry] : input[entry.when] ? [entry.id] : []
    )
    if (rows.length === 0) continue
    if (out.length > 0) out.push(`separator`)
    out.push(...rows)
  }
  return out
}

/** THE New list for the viewer: a chat for every member, the rest owners'. */
export function sidebarNewKinds(input: { owner: boolean }): SidebarNewKind[] {
  const ownerOnly = fixture.newOwnerOnly as string[]
  return SIDEBAR_NEW_KINDS.filter((kind) => input.owner || !ownerOnly.includes(kind))
}

export interface NewTabMenuRow {
  /** `assistant`, `separator`, or `team:<id>`. */
  id: string
  children: SidebarNewKind[]
}

/** THE new-tab menu (the tab strip's +): the Assistant, then one submenu per
 *  team (already in sidebar order) holding THE New list. */
export function newTabMenu(input: { assistant: boolean; teams: readonly { id: string; owner: boolean }[] }): NewTabMenuRow[] {
  const teams = input.teams.map((team) => ({ id: `team:${team.id}`, children: sidebarNewKinds(team) }))
  if (!input.assistant) return teams
  return [
    { id: `assistant`, children: [] },
    ...(teams.length > 0 ? [{ id: `separator`, children: [] }] : []),
    ...teams,
  ]
}

/** What a phone's menu leaves out (`phone.drops`). */
export const SIDEBAR_PHONE_DROPS = fixture.phone.drops as readonly SidebarMenuEntryId[]

/** #EXP-2109: a row's long-press menu on a phone: the same table, minus
 *  `phone.drops`; Open alone = no menu. */
export function sidebarPhoneMenu(kind: SidebarMenuKind, input: SidebarMenuInput = {}): SidebarMenuEntryId[] {
  const menu = sidebarItemMenu(kind, input)
    .filter((id) => !SIDEBAR_PHONE_DROPS.includes(id))
    .filter((id, index, all) => id !== `separator` || (index > 0 && index < all.length - 1 && all[index - 1] !== `separator`))
  return menu.length === 1 && menu[0] === `open` ? [] : menu
}

export interface SidebarMoveTargets {
  /** The folders it can move into (not the one holding it). */
  folders: { id: string; name: string | null }[]
  /** Inside a folder: that folder (Top level = `after` it); else null. */
  topLevel: string | null
}

/** Move to folder › over the shown rows (a team's `sidebarRows`, depth 0 =
 *  its top level); New folder always stands between the two. */
export function sidebarMoveTargets(
  rows: readonly { kind: string; id: string; depth: number; name: string | null }[],
  id: string
): SidebarMoveTargets {
  let topLevel: string | null = null
  let holder: string | null = null
  for (const row of rows) {
    if (row.kind === `folder`) holder = row.id
    else if (row.depth === 0) holder = null
    if (row.id === id) topLevel = row.kind === `folder` ? null : holder
  }
  return {
    folders: rows.filter((row) => row.kind === `folder` && row.id !== topLevel).map(({ id, name }) => ({ id, name })),
    topLevel,
  }
}
