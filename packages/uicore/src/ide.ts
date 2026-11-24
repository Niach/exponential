// #EXP-1725: what the IDE's shell regions show, a pure projection of the
// host's state. `fixtures/ide-sidebar.json` is the spec; the document
// (`apps/ede/ui/sidebar.express`) binds the result under /shell/ide.
import fixture from "../fixtures/ide-sidebar.json" with { type: "json" }
// #EXP-1832: the folder tree is domain-contract's (sidebar-folders.json), by
// path: uicore takes no package dependencies.
import { SIDEBAR_FOLDER_DEFAULT_NAME, sidebarRows, sidebarTree, type SidebarEntry, type SidebarNode } from "../../domain-contract/src/sidebar-folders"

export type IdeDot = `amber` | `green` | `primary`

/** #FEED-232 (`nav-badges.json`): a nav row's count badge tone; no dots. */
export type IdeTone = `attention` | `muted`

/** A count the host computed (`chatsBadge`): 0 = no badge. */
export interface IdeCount {
  count: number
  tone: IdeTone
}

/** #EXP-2027 (domain-contract sidebar-tree.json): one item of a team's mixed
 *  list in the host's default order (`sidebarTeamItems`). */
export interface IdeItemInput {
  kind: `board` | `agent` | `app` | `session`
  id: string
  name: string
  icon: string
  /** An app's slug (its row lights on it too). */
  slug?: string
  /** An agent: needs you / its conversation's unread agent messages. */
  needsYou?: boolean
  unread?: number
  /** A session: every session of its tree (its row lights on any). */
  sessionIds?: string[]
}

export interface IdeTeamInput {
  id: string
  name: string
  /** The team mark the host paints. */
  mark: string
  /** Device-local expanded flag (`openKeys.team`; absent = open). */
  open?: boolean
  /** Owners get New › Board | Agent | App and the board gears. */
  owner?: boolean
  /** The team's needs-you count (attention). */
  needsYou?: number
  items: IdeItemInput[]
  /** The member's `sidebar_layout` rows of the team's `team` list. */
  layout?: SidebarEntry[]
}

export interface IdeSidebarInput {
  /** The open screen as `<kind>` or `<kind>:<id>`. */
  screen: string
  /** #EXP-2124: the viewer's Assistant, the first row (none = no row). */
  assistant?: { id: string; name: string; unread?: number; needsYou?: boolean } | null
  needsYou: number
  drafts: number
  chats: IdeCount
  /** #EXP-2027: the Devices row's children in `sidebarDeviceRows` order, and whether it is open. */
  devices?: { id: string; name: string; icon: string; online: boolean; teamMark?: string | null }[]
  devicesOpen?: boolean
  /** #EXP-2027: one entry per team (`sidebarTeams`). */
  teams?: IdeTeamInput[]
  active: { id: string; title: string; icon: string; sessionIds: string[]; unread: boolean; teamMark?: string | null; team?: string | null }[]
  /** #EXP-1832: the folders this device keeps folded (none = open). */
  closed?: string[]
  /** #EXP-1832: the folder whose name is being edited in place. */
  editingFolder?: string | null
  /** #EXP-1616: the Exponential repositories, each with its live PR tree (`repoPrTree`, placed by the host). */
  repos?: { id: string; name: string; prs: { id: string; identifier: string; title: string; subtitle: string; depth: number; session?: string | null }[] }[]
}

/** #EXP-1616: a Pull requests tree row: a repository at depth 0, its PRs under it. */
export interface IdePrRow {
  id: string
  repo: string
  title: string
  identifier: string | null
  subtitle: string | null
  icon: string | null
  depth: number
  selected: boolean
}

export interface IdeNavRow {
  id: string
  title: string
  icon: string
  selected: boolean
  count: string | null
  tone: IdeTone | null
}

export interface IdeMenuEntry {
  label: string
  value: string
  icon: string
}

/** #EXP-2027: one row of a team's List: the team row (depth 0), a folder
 *  (depth 1) or an item (depth 1, or 2 inside a folder). */
export interface IdeRow {
  kind: `team` | `item` | `folder`
  item: IdeItemInput[`kind`] | null
  id: string
  depth: 0 | 1 | 2
  group: boolean
  /** A team's or folder's expanded flag (false on an item). */
  open: boolean
  title: string
  icon: string
  selected: boolean
  count: string | null
  tone: IdeTone | null
  /** The hover gear: agents and apps for every member, boards for owners. */
  gear: boolean
  /** A FOLDED folder's item count; null otherwise. */
  meta: string | null
  /** A team's or folder's fold glyph: ui-chevron-down open, ui-chevron-right folded. */
  foldIcon: string | null
  /** The folder's name is an Input in place. */
  editing: boolean
  /** The team row's New submenu (empty elsewhere). */
  newItems: IdeMenuEntry[]
}

export interface IdeDeviceRow {
  kind: `devices` | `device`
  id: string
  depth: 0 | 1
  group: boolean
  open: boolean
  title: string
  icon: string
  selected: boolean
  /** Offline: drawn muted. */
  muted: boolean
  /** A teammate's server: its team's mark; an online device: the green dot. */
  mark: string | null
  foldIcon: string | null
}

type IdeRowBase = Pick<IdeRow, `item` | `id` | `title` | `icon` | `selected` | `count` | `tone` | `gear`>

const foldIcon = (open: boolean) => (open ? `ui-chevron-down` : `ui-chevron-right`)

/** `sidebarTree`, plus the folder being named while it holds no item yet
 *  (sidebar-tree.json New folder), placed among the placed nodes by its
 *  sortOrder. */
function teamTree(ids: string[], entries: readonly SidebarEntry[], editing: string | null): SidebarNode[] {
  const tree = sidebarTree(ids, entries)
  const folder = entries.find((e) => e.kind === `folder` && e.id === editing)
  if (!folder || tree.some((n) => n.id === folder.id)) return tree
  const key = (node: SidebarNode) =>
    node.kind === `folder` ? entries.find((e) => e.kind === `folder` && e.id === node.id) : [...entries].sort(compareEntries).find((e) => e.kind === `item` && e.targetId === node.id)
  const at = tree.findIndex((node) => {
    const entry = key(node)
    return !entry || compareEntries(entry, folder) > 0
  })
  const empty: SidebarNode = { kind: `folder`, id: folder.id, name: folder.name ?? SIDEBAR_FOLDER_DEFAULT_NAME, children: [] }
  return at < 0 ? [...tree, empty] : [...tree.slice(0, at), empty, ...tree.slice(at)]
}

const compareEntries = (a: SidebarEntry, b: SidebarEntry) => (a.sortOrder !== b.sortOrder ? a.sortOrder - b.sortOrder : a.id < b.id ? -1 : a.id > b.id ? 1 : 0)

/** A team's rows as the member arranged them, one level under the team
 *  row: a folded folder's items left out. */
export function folderRows(rows: IdeRowBase[], entries: readonly SidebarEntry[], closed: readonly string[], editing: string | null): IdeRow[] {
  const byId = new Map(rows.map((row) => [row.id, row]))
  const shut = new Set(closed)
  return sidebarRows(teamTree(rows.map((row) => row.id), entries, editing), (id) => !shut.has(id)).map((r): IdeRow => {
    const node = { kind: r.kind, id: r.id, depth: (r.depth + 1) as 1 | 2, group: r.group, open: r.open, newItems: [] }
    if (r.kind === `item`) return { ...node, ...(byId.get(r.id) as IdeRowBase), meta: null, foldIcon: null, editing: false }
    return {
      ...node,
      item: null,
      title: r.name ?? ``,
      icon: FOLDER_ICON,
      selected: false,
      count: null,
      tone: null,
      gear: false,
      meta: r.open ? null : String(r.count),
      foldIcon: foldIcon(r.open),
      editing: editing === r.id,
    }
  })
}

const FOLDER_ICON = `ui-folder`

export interface IdeSidebar {
  nav: IdeNavRow[]
  devices: IdeDeviceRow[]
  teams: { id: string; rows: IdeRow[] }[]
  active: { id: string; title: string; icon: string; selected: boolean; unread: boolean; teamMark: string | null }[]
  prs: IdePrRow[]
}

export interface IdeTabInput {
  key: string
  title: string
  icon: string
}

export interface IdeTabs {
  tabs: { value: string; label: string; icon: string }[]
  activeTab: string | null
}

const COPY = fixture.copy

/** The badge's label: null at zero, `99+` past 99 (nav-badges.json `labelCases`). */
export const countLabel = (count: number): string | null => count <= 0 ? null : count > 99 ? `99+` : String(count)

const badge = (count: number, tone: IdeTone) => {
  const label = countLabel(count)
  return { count: label, tone: label === null ? null : tone }
}

const nav = (id: keyof typeof COPY, icon: string, selected: boolean, count: number, tone: IdeTone): IdeNavRow => ({
  id,
  title: COPY[id],
  icon,
  selected,
  ...badge(count, tone),
})

/** An agent row's badge (`rowBadge`): its unread, at least 1 while it needs
 *  you, attention when it does. Busy is no badge. */
export const agentCount = (unread: number, needsYou: boolean): IdeCount =>
  needsYou ? { count: Math.max(1, unread), tone: `attention` } : { count: unread, tone: `muted` }

/** THE New list (#EXP-2124, domain-contract sidebar-menus.json `newKinds`: a chat for every member, then agent, app, board for owners). */
export const newItems = (owner: boolean): IdeMenuEntry[] => [
  { label: COPY.session, value: `session`, icon: `action-chat` },
  ...(owner
    ? [
        { label: COPY.agent, value: `agent`, icon: `settings-agents` },
        { label: COPY.app, value: `app`, icon: `nav-apps` },
        { label: COPY.board, value: `board`, icon: `nav-boards` },
      ]
    : []),
]

export function ideSidebar(input: IdeSidebarInput): IdeSidebar {
  const { screen } = input
  const on = (kind: string, id: string) => screen === `${kind}:${id}`
  const rows: IdeNavRow[] = []
  if (input.assistant) {
    const { count, tone } = agentCount(input.assistant.unread ?? 0, input.assistant.needsYou ?? false)
    rows.push({ id: `assistant`, title: input.assistant.name, icon: `brand-exponential`, selected: on(`agent`, input.assistant.id), ...badge(count, tone) })
  }
  rows.push(nav(`home`, `nav-home`, screen === `home`, 0, `muted`))
  if (input.needsYou > 0 || screen === `needs-you`) rows.push(nav(`needs-you`, `nav-needs-you`, screen === `needs-you`, input.needsYou, `attention`))
  rows.push(nav(`chats`, `nav-chats`, screen === `chats`, input.chats.count, input.chats.tone))
  if (input.drafts > 0 || screen === `drafts`) rows.push(nav(`drafts`, `nav-drafts`, screen === `drafts`, input.drafts, `muted`))
  const devicesOpen = input.devicesOpen ?? true
  const devices: IdeDeviceRow[] = [
    { kind: `devices`, id: `devices`, depth: 0, group: true, open: devicesOpen, title: COPY.devices, icon: `nav-devices`, selected: false, muted: false, mark: null, foldIcon: foldIcon(devicesOpen) },
    ...(devicesOpen ? input.devices ?? [] : []).map((d): IdeDeviceRow => ({
      kind: `device`,
      id: d.id,
      depth: 1,
      group: false,
      open: false,
      title: d.name,
      icon: d.icon,
      selected: on(`device`, d.id) || on(`device-settings`, d.id),
      muted: !d.online,
      mark: d.teamMark ?? (d.online ? `ide:dot:green` : null),
      foldIcon: null,
    })),
  ]
  const teams = (input.teams ?? []).map((team) => {
    const open = team.open ?? true
    const owner = team.owner ?? false
    const items = team.items.map((item): IdeRowBase => {
      const count = item.kind === `agent` ? badge(agentCount(item.unread ?? 0, item.needsYou ?? false).count, agentCount(item.unread ?? 0, item.needsYou ?? false).tone) : { count: null, tone: null }
      const selected =
        item.kind === `app`
          ? on(`app`, item.id) || (item.slug !== undefined && on(`app`, item.slug))
          : item.kind === `session`
            ? on(`chat`, item.id) || on(`session`, item.id) || (item.sessionIds ?? []).some((id) => on(`session`, id))
            : on(item.kind, item.id)
      const gear = item.kind === `agent` || item.kind === `app` || (item.kind === `board` && owner)
      return { item: item.kind, id: item.id, title: item.name, icon: item.icon, selected, ...count, gear }
    })
    const head: IdeRow = {
      kind: `team`,
      item: null,
      id: team.id,
      depth: 0,
      group: true,
      open,
      title: team.name,
      icon: team.mark,
      selected: false,
      ...badge(team.needsYou ?? 0, `attention`),
      gear: false,
      meta: null,
      foldIcon: foldIcon(open),
      editing: false,
      newItems: newItems(owner),
    }
    return { id: team.id, rows: open ? [head, ...folderRows(items, team.layout ?? [], input.closed ?? [], input.editingFolder ?? null)] : [head] }
  })
  return {
    nav: rows,
    devices,
    teams,
    active: input.active.map((a) => ({
      id: a.id,
      title: a.title,
      icon: a.icon,
      selected: on(`chat`, a.id) || a.sessionIds.some((id) => on(`session`, id)),
      unread: a.unread,
      teamMark: a.teamMark ?? null,
    })),
    prs: (input.repos ?? []).filter((r) => r.prs.length > 0).flatMap((r): IdePrRow[] => [
      { id: r.id, repo: r.id, title: r.name, identifier: null, subtitle: null, icon: `ui-repository`, depth: 0, selected: on(`repo`, r.id) },
      ...r.prs.map((p) => ({ id: p.id, repo: r.id, title: p.title, identifier: p.identifier, subtitle: p.subtitle, icon: null, depth: p.depth + 1, selected: p.session != null && (on(`chat`, p.session) || on(`session`, p.session)) })),
    ]),
  }
}

/** The tab bar: every open tab in tab order, the active one by key. */
export function ideTabs(tabs: IdeTabInput[], active: string | null): IdeTabs {
  return {
    tabs: tabs.map((t) => ({ value: t.key, label: t.title, icon: t.icon })),
    activeTab: tabs.some((t) => t.key === active) ? active : null,
  }
}

export type IdeTabPlacement = { placement: "top" | "bottom"; moreSide: "top" | "bottom" }

/** Where the tab bar sits: on top in the standalone IDE, at the bottom inside Exponential Desktop; +N opens away from the edge. */
export function ideTabPlacement(ede: boolean): IdeTabPlacement {
  return ede ? { placement: "bottom", moreSide: "top" } : { placement: "top", moreSide: "bottom" }
}
