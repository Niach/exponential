// #EXP-2124 DRAFT (web only until the plan is approved; the build moves this
// table into `fixtures/sidebar-menus.json` + `src/sidebar-menus.ts` of
// @exp/domain-contract, replayed ×4): ONE menu model per sidebar item kind.
//
// Every menu is the same five groups in the same order, a separator between
// the non-empty ones:
//
//   open      Open, Open in new tab
//   kind      what only this kind does (New ›, New issue, Compact, …)
//   arrange   Rename, Copy link, Move to folder ›, Add to / Remove from sidebar
//   settings  the item's settings page
//   danger    destructive, last (Archive, Delete, Remove, Leave)

export const SIDEBAR_MENU_KINDS = [
  `assistant`,
  `home`,
  `needs-you`,
  `chats`,
  `drafts`,
  `devices`,
  `device`,
  `teams`,
  `team`,
  `folder`,
  `board`,
  `app`,
  `agent`,
  `session`,
  `active-session`,
  `getting-started`,
] as const
export type SidebarMenuKind = (typeof SIDEBAR_MENU_KINDS)[number]

export type SidebarMenuAction =
  | `open`
  | `open-new-tab`
  | `new-chat`
  | `new`
  | `new-folder`
  | `new-issue`
  | `new-team`
  | `join-team`
  | `add-device`
  | `compact`
  | `new-conversation`
  | `edit`
  | `rename`
  | `copy-link`
  | `move`
  | `ungroup`
  | `add-to-sidebar`
  | `remove-from-sidebar`
  | `settings`
  | `archive`
  | `delete`
  | `remove`
  | `leave`

export type SidebarMenuEntryId = SidebarMenuAction | `separator`

export interface SidebarMenuInput {
  /** The viewer owns the item's team. */
  owner?: boolean
  /** A device row: the viewer's own device (not a teammate's server). */
  mine?: boolean
  /** A session row: the viewer may delete it (its starter or an owner). */
  canDelete?: boolean
}

export const SIDEBAR_MENU_COPY: Record<SidebarMenuAction, string> = {
  open: `Open`,
  "open-new-tab": `Open in new tab`,
  "new-chat": `New chat`,
  new: `New`,
  "new-folder": `New folder`,
  "new-issue": `New issue`,
  "new-team": `New team`,
  "join-team": `Join team`,
  "add-device": `Add device`,
  compact: `Compact`,
  "new-conversation": `New conversation`,
  edit: `Edit`,
  rename: `Rename`,
  "copy-link": `Copy link`,
  move: `Move to folder`,
  ungroup: `Ungroup`,
  "add-to-sidebar": `Add to sidebar`,
  "remove-from-sidebar": `Remove from sidebar`,
  settings: `Settings`,
  archive: `Archive`,
  delete: `Delete`,
  remove: `Remove device`,
  leave: `Leave team`,
}

/** The shortcuts that exist elsewhere, shown on the same verb here. */
export const SIDEBAR_MENU_SHORTCUTS: Partial<Record<SidebarMenuAction, { apple: string; other: string }>> = {
  "new-chat": { apple: `⌥T`, other: `Alt+T` },
}

export const SIDEBAR_MENU_DESTRUCTIVE: ReadonlySet<SidebarMenuAction> = new Set([`archive`, `delete`, `remove`, `leave`])

const OPEN = [`open`, `open-new-tab`] as const

type Groups = readonly (readonly SidebarMenuAction[])[]

function groupsOf(kind: SidebarMenuKind, input: SidebarMenuInput): Groups {
  const owner = input.owner ?? false
  switch (kind) {
    case `assistant`:
      return [OPEN, [`compact`, `new-conversation`], [], [`settings`], []]
    case `home`:
    case `chats`:
      return [OPEN, [`new-chat`], [], [], []]
    case `needs-you`:
    case `drafts`:
      return [OPEN, [], [], [], []]
    case `devices`:
      return [[], [`add-device`], [], [], []]
    case `device`:
      return [OPEN, [], input.mine ? [`rename`, `copy-link`] : [`copy-link`], [`settings`], input.mine ? [`remove`] : []]
    case `teams`:
      return [[], [`new-team`, `join-team`], [], [], []]
    case `team`:
      return [[], [`new`, `new-folder`], [], [`settings`], [`leave`]]
    case `folder`:
      return [[], [], [`rename`, `ungroup`], [], []]
    case `board`:
      return [OPEN, [`new-issue`], [`copy-link`, `move`], owner ? [`settings`] : [], owner ? [`archive`] : []]
    case `app`:
      return [OPEN, owner ? [`edit`] : [], [`copy-link`, `move`], owner ? [`settings`] : [], owner ? [`delete`] : []]
    case `agent`:
      return [OPEN, [`compact`, `new-conversation`], [`move`], owner ? [`settings`] : [], owner ? [`delete`] : []]
    case `session`:
      return [OPEN, [], [`copy-link`, `move`, `remove-from-sidebar`], [], input.canDelete ? [`delete`] : []]
    case `active-session`:
      return [OPEN, [], [`copy-link`, `add-to-sidebar`], [], input.canDelete ? [`delete`] : []]
    case `getting-started`:
      return [[`open`], [], [], [], []]
  }
}

/** A kind's menu for the viewer: the groups in order, `separator` between
 *  the non-empty ones. */
export function sidebarItemMenu(kind: SidebarMenuKind, input: SidebarMenuInput = {}): SidebarMenuEntryId[] {
  const out: SidebarMenuEntryId[] = []
  for (const group of groupsOf(kind, input)) {
    if (group.length === 0) continue
    if (out.length > 0) out.push(`separator`)
    out.push(...group)
  }
  return out
}

/** THE new-tab menu (the tab strip's +): the Assistant, then one submenu per
 *  team in sidebar order: a chat for every member, agent / app / board for
 *  an owner (settings-nav.json `newEntries`). */
export const NEW_TAB_KINDS = [`session`, `agent`, `app`, `board`] as const
export type NewTabKind = (typeof NEW_TAB_KINDS)[number]

export const NEW_TAB_COPY: Record<NewTabKind, string> = {
  session: `Chat`,
  agent: `Agent`,
  app: `App`,
  board: `Board`,
}

export function newTabTeamMenu(input: { owner: boolean }): NewTabKind[] {
  return NEW_TAB_KINDS.filter((kind) => kind === `session` || input.owner)
}
