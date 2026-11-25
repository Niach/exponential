// #EXP-1725: what the IDE's shell regions show, a pure projection of the
// host's state. `fixtures/ide-sidebar.json` is the spec; the document
// (`apps/ede/ui/sidebar.express`) binds the result under /shell/ide.
import fixture from "../fixtures/ide-sidebar.json" with { type: "json" }
// The badges (nav-badges.json) and the folder tree (#EXP-1832
// sidebar-folders.json) are domain-contract's, by path: uicore takes no
// package dependencies.
import { countBadgeLabel, rowBadge, sumBadges, teamBadge, type CountBadgeTone, type CountBadgeValue } from "../../domain-contract/src/nav-badges"
import { SIDEBAR_FOLDER_DEFAULT_NAME, sidebarRows, sidebarTree, type SidebarEntry, type SidebarNode } from "../../domain-contract/src/sidebar-folders"

/** #FEED-232 (`nav-badges.json`): a nav row's count badge tone; no dots. */
export type IdeTone = CountBadgeTone

/** A count the host computed (`chatsBadge`): 0 = no badge. */
export type IdeCount = CountBadgeValue

/** #EXP-2027 (domain-contract sidebar-tree.json): one item of a team's mixed
 *  list in the host's default order (`sidebarTeamItems`). */
export interface IdeItemInput {
  kind: `board` | `agent` | `app` | `session`
  id: string
  name: string
  icon: string
  /** An app's slug (its row lights on it too). */
  slug?: string
  /** An agent or a placed session: needs you / its unread agent messages. */
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
  /** The team's share of Needs you (`needsYouCounts(...).teams`). */
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
  active: { id: string; title: string; icon: string; sessionIds: string[]; teamMark?: string | null; team?: string | null }[]
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
  /** A FOLDED folder's item count; null otherwise (its count badge sits left of it). */
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
  /** The viewer's own online device: the green dot. */
  mark: string | null
  /** A teammate's server: the team's mark, on the device glyph (nav-badges.json THE TEAM MARK ON A ROW). */
  teamMark: string | null
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
export function folderRows(rows: IdeRowBase[], counts: ReadonlyMap<string, IdeCount>, entries: readonly SidebarEntry[], closed: readonly string[], editing: string | null): IdeRow[] {
  const byId = new Map(rows.map((row) => [row.id, row]))
  const shut = new Set(closed)
  const tree = teamTree(rows.map((row) => row.id), entries, editing)
  const hidden = new Map(tree.flatMap((n) => (n.kind === `folder` && shut.has(n.id) ? [[n.id, sumBadges(n.children.flatMap((id) => counts.get(id) ?? []))] as const] : [])))
  return sidebarRows(tree, (id) => !shut.has(id)).map((r): IdeRow => {
    const node = { kind: r.kind, id: r.id, depth: (r.depth + 1) as 1 | 2, group: r.group, open: r.open, newItems: [] }
    if (r.kind === `item`) return { ...node, ...(byId.get(r.id) as IdeRowBase), meta: null, foldIcon: null, editing: false }
    return {
      ...node,
      item: null,
      title: r.name ?? ``,
      icon: FOLDER_ICON,
      selected: false,
      ...badge(hidden.get(r.id) ?? NONE),
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
  active: { id: string; title: string; icon: string; selected: boolean; teamMark: string | null }[]
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

const NONE: IdeCount = { count: 0, tone: `muted` }

const badge = ({ count, tone }: IdeCount): { count: string | null; tone: IdeTone | null } => {
  const label = countBadgeLabel(count)
  return label ? { count: label, tone } : { count: null, tone: null }
}

const nav = (id: keyof typeof COPY, icon: string, selected: boolean, count: number, tone: IdeTone): IdeNavRow => ({
  id,
  title: COPY[id],
  icon,
  selected,
  ...badge({ count, tone }),
})

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
    rows.push({ id: `assistant`, title: input.assistant.name, icon: `brand-exponential`, selected: on(`agent`, input.assistant.id), ...badge(rowBadge({ unreadCount: input.assistant.unread ?? 0, needsYou: input.assistant.needsYou ?? false })) })
  }
  rows.push(nav(`home`, `nav-home`, screen === `home`, 0, `muted`))
  if (input.needsYou > 0 || screen === `needs-you`) rows.push(nav(`needs-you`, `nav-needs-you`, screen === `needs-you`, input.needsYou, `attention`))
  rows.push(nav(`chats`, `nav-chats`, screen === `chats`, input.chats.count, input.chats.tone))
  if (input.drafts > 0 || screen === `drafts`) rows.push(nav(`drafts`, `nav-drafts`, screen === `drafts`, input.drafts, `muted`))
  const devicesOpen = input.devicesOpen ?? true
  const devices: IdeDeviceRow[] = [
    { kind: `devices`, id: `devices`, depth: 0, group: true, open: devicesOpen, title: COPY.devices, icon: `nav-devices`, selected: false, muted: false, mark: null, teamMark: null, foldIcon: foldIcon(devicesOpen) },
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
      mark: d.online && !d.teamMark ? `ide:dot:green` : null,
      teamMark: d.teamMark ?? null,
      foldIcon: null,
    })),
  ]
  const teams = (input.teams ?? []).map((team) => {
    const open = team.open ?? true
    const owner = team.owner ?? false
    const counts = new Map(team.items.filter((i) => i.kind === `agent` || i.kind === `session`).map((i) => [i.id, rowBadge({ unreadCount: i.unread ?? 0, needsYou: i.needsYou ?? false })]))
    const items = team.items.map((item): IdeRowBase => {
      const selected =
        item.kind === `app`
          ? on(`app`, item.id) || (item.slug !== undefined && on(`app`, item.slug))
          : item.kind === `session`
            ? on(`chat`, item.id) || on(`session`, item.id) || (item.sessionIds ?? []).some((id) => on(`session`, id))
            : on(item.kind, item.id)
      const gear = item.kind === `agent` || item.kind === `app` || (item.kind === `board` && owner)
      return { item: item.kind, id: item.id, title: item.name, icon: item.icon, selected, ...badge(counts.get(item.id) ?? NONE), gear }
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
      ...badge(teamBadge({ needsYou: team.needsYou ?? 0, open, rows: [...counts.values()] })),
      gear: false,
      meta: null,
      foldIcon: foldIcon(open),
      editing: false,
      newItems: newItems(owner),
    }
    return { id: team.id, rows: open ? [head, ...folderRows(items, counts, team.layout ?? [], input.closed ?? [], input.editingFolder ?? null)] : [head] }
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
