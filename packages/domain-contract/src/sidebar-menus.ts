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
